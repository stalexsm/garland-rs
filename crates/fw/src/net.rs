//! Сеть Гирлянды: Wi-Fi STA (esp-radio), стек embassy-net с DHCP и именем
//! `garland`, HTTP-сервер страницы версии Прошивки.
//!
//! Каждая часть — отдельная embassy-задача, не зависящая от задачи Ленты:
//! сбой или недоступность сети кончаются повторной попыткой и не касаются
//! рендера (эффекты идут при любой сети).

use crate::http;
use crate::ota;
use crate::{WIFI_PASS, WIFI_SSID};
use alloc::boxed::Box;
use embassy_executor::Spawner;
use embassy_net::tcp::TcpSocket;
use embassy_net::{DhcpConfig, Runner, Stack, StackResources};
use embassy_time::{Duration, Timer};
use esp_bootloader_esp_idf::partitions::Error as PartitionError;
use esp_hal::system::software_reset;
use esp_radio::wifi::{
    AuthenticationMethodConfig, Config, ControllerConfig, Interface, WifiController,
    sta::StationConfig,
};
use esp_storage::FlashStorage;
use log::{error, info, warn};

/// Имя Гирлянды в домашней сети: видно в роутере (DHCP Option 12).
pub const HOSTNAME: &str = "garland";

/// Порт HTTP-сервера.
const HTTP_PORT: u16 = 80;

/// Пауза между попытками подключения Wi-Fi, мс (канон примера esp-hal).
const WIFI_RETRY_MS: u64 = 5000;

/// TCP-таймаут сокета сервера, мс: зависший клиент не держит соединение.
const TCP_TIMEOUT_MS: u64 = 10_000;

/// Пауза после неудачного accept, мс.
const ACCEPT_RETRY_MS: u64 = 1000;

/// Размер буфера заголовка запроса, байт: страница и заливка идут из
/// браузера с длинными заголовками fetch, переполнение отвечает 400.
const HEAD_BUF_SIZE: usize = 2048;

/// Размеры TCP-буферов сокета, байт: приёмный — с запасом на паузы
/// во время флешных операций заливки (стирание сектора блокирует
/// обработку пакетов), исходный — страница и короткие ответы OTA.
const SOCKET_RX_SIZE: usize = 4096;
const SOCKET_TX_SIZE: usize = 1024;

/// Пауза перед перезагрузкой после принятого образа, мс: ответ об
/// успехе должен успеть уйти клиенту.
const OTA_REBOOT_DELAY_MS: u64 = 1000;

/// Слоты сокетов стека: DHCP + TCP-сервер (канон примера esp-hal — 3).
const STACK_SOCKETS: usize = 3;

/// Поднять сеть: контроллер Wi-Fi со сборочными учётными данными, стек
/// embassy-net (DHCP + [`HOSTNAME`]) и задачи — коннектор, драйвер стека и
/// HTTP-сервер. Вызывается из `main`, только когда учётные данные заданы.
/// `flash` — отдельный экземпляр флеша для OTA-заливки (NVS живёт в задаче
/// Ленты).
// large_stack_frames меряет разовую инициализацию (конфиги Wi-Fi и стека);
// кадр живёт только до первого spawn, задачи дальше работают без него.
#[expect(
    clippy::large_stack_frames,
    reason = "разовая инициализация сети держит конфиги на кадре; дальше их не существует"
)]
pub fn start(
    spawner: &Spawner,
    wifi: esp_hal::peripherals::WIFI<'static>,
    seed: u64,
    version: &'static str,
    flash: FlashStorage<'static>,
) {
    let station_config = Config::Station(
        StationConfig::default()
            .with_ssid(WIFI_SSID.try_into().expect("WIFI_SSID: длиннее 32 байт"))
            // WPA/WPA2: покрывает домашние роутеры в смешанном режиме;
            // сети только-WPA3 esp-radio beta не поддерживает.
            .with_authentication(AuthenticationMethodConfig::WpaWpa2Personal(
                WIFI_PASS.try_into().expect("WIFI_PASS: длиннее 64 байт"),
            )),
    );
    let controller = WifiController::new(
        wifi,
        ControllerConfig::default().with_initial_config(station_config),
    )
    .expect("Wi-Fi: контроллер не поднялся");

    let mut dhcp = DhcpConfig::default();
    dhcp.hostname = Some(HOSTNAME.try_into().expect("hostname: длиннее 32 байт"));
    let resources: &mut StackResources<STACK_SOCKETS> = Box::leak(Box::new(StackResources::new()));
    let (stack, runner) = embassy_net::new(
        Interface::station(),
        embassy_net::Config::dhcpv4(dhcp),
        resources,
        seed,
    );

    spawner.spawn(net_task(runner).expect("задача сетевого стека не поднялась"));
    spawner.spawn(connection_task(controller).expect("задача Wi-Fi не поднялась"));
    spawner.spawn(http_task(stack, version, flash).expect("задача HTTP-сервера не поднялась"));
    info!("Wi-Fi: SSID «{WIFI_SSID}», имя в сети — «{HOSTNAME}»");
}

/// Драйвер сетевого стека: гоняет пакеты между Wi-Fi и embassy-net.
#[embassy_executor::task]
async fn net_task(mut runner: Runner<'static, Interface>) {
    runner.run().await
}

/// Подключение станции: попытка → работа до разрыва → пауза → заново.
/// Любой сбой сети заканчивается повторной попыткой через [`WIFI_RETRY_MS`].
// large_stack_frames меряет фьючерс embassy-задачи, а embassy выделяет его
// статически (bss), стек при работе не расходуется; ожидание (expect) этот
// поздний линт не связывает, поэтому прицельное разрешение (allow).
#[allow(
    clippy::large_stack_frames,
    reason = "фьючерс embassy-задачи выделяется статически (bss), стек не расходуется"
)]
#[embassy_executor::task]
async fn connection_task(mut controller: WifiController<'static>) {
    loop {
        info!("Wi-Fi: подключение…");
        match controller.connect_async().await {
            Ok(connected) => {
                info!("Wi-Fi: подключено к «{}»", connected.ssid.as_str());
                if let Ok(disconnected) = controller.wait_for_disconnect_async().await {
                    info!(
                        "Wi-Fi: разрыв (причина {:?}), переподключение",
                        disconnected.reason
                    );
                }
            }
            Err(e) => error!("Wi-Fi: не удалось подключиться: {e:?}"),
        }
        Timer::after(Duration::from_millis(WIFI_RETRY_MS)).await;
    }
}

/// HTTP-сервер: ждёт адрес по DHCP, затем принимает соединения по одному.
/// Владеет экземпляром флеша для OTA-заливки.
// large_stack_frames меряет фьючерс задачи (буферы сокета, страницы и OTA);
// embassy выделяет его статически (bss), стек при работе не расходуется.
#[allow(
    clippy::large_stack_frames,
    reason = "фьючерс embassy-задачи выделяется статически (bss), стек не расходуется"
)]
#[embassy_executor::task]
async fn http_task(stack: Stack<'static>, version: &'static str, mut flash: FlashStorage<'static>) {
    stack.wait_config_up().await;
    info!("HTTP: адрес получен, сервер на порту {HTTP_PORT}");
    // Буферы сокета — в куче один раз: сокет пересоздаётся на каждое
    // соединение, буферы живут столько же, сколько задача.
    let rx: &'static mut [u8; SOCKET_RX_SIZE] = Box::leak(Box::new([0; SOCKET_RX_SIZE]));
    let tx: &'static mut [u8; SOCKET_TX_SIZE] = Box::leak(Box::new([0; SOCKET_TX_SIZE]));
    // Буферы запроса, ответа и OTA-сессии живут во фьючерсе задачи (bss).
    let mut head = [0u8; HEAD_BUF_SIZE];
    let mut response = heapless::String::<{ http::RESPONSE_MAX }>::new();
    let mut ota = ota::OtaBuffers::leak();
    loop {
        let mut socket = TcpSocket::new(stack, rx, tx);
        socket.set_timeout(Some(Duration::from_millis(TCP_TIMEOUT_MS)));
        if let Err(e) = socket.accept(HTTP_PORT).await {
            warn!("HTTP: accept не удался: {e:?}");
            Timer::after(Duration::from_millis(ACCEPT_RETRY_MS)).await;
            continue;
        }
        serve(
            &mut socket,
            version,
            &mut head,
            &mut response,
            &mut flash,
            &mut ota,
        )
        .await;
    }
}

/// Обработать одно соединение: прочитать заголовок, ответить и закрыть.
// Вложенный фьючерс задачи: живёт в статическом пуле embassy (bss),
// стек при работе не расходуется.
#[allow(
    clippy::large_stack_frames,
    reason = "вложенный фьючерс embassy-задачи выделяется статически (bss), стек не расходуется"
)]
async fn serve(
    socket: &mut TcpSocket<'_>,
    version: &str,
    head: &mut [u8],
    response: &mut heapless::String<{ http::RESPONSE_MAX }>,
    flash: &mut FlashStorage<'static>,
    ota: &mut ota::OtaBuffers,
) {
    match read_head(socket, head).await {
        // Хвост буфера за `n` может содержать мусор прошлых запросов —
        // и разбор, и тело образа режутся по реально прочитанному.
        Head::Complete(n) => match http::parse_request(&head[..n]) {
            Some(http::Request::GetRoot) => {
                http::write_page(response, version);
                send(socket, response.as_bytes()).await;
            }
            Some(http::Request::PostOta { content_length }) => {
                receive_image(socket, content_length, &head[..n], response, flash, ota).await;
            }
            Some(http::Request::Other { method, path }) => {
                info!("HTTP: нет маршрута {method} {path}");
                send(socket, http::RESPONSE_NOT_FOUND.as_bytes()).await;
            }
            Some(http::Request::Malformed) => {
                warn!("HTTP: запрос не разбирается");
                send(socket, http::RESPONSE_BAD_REQUEST.as_bytes()).await;
            }
            None => warn!("HTTP: соединение закрыто без полного заголовка"),
        },
        Head::Overflow => {
            warn!("HTTP: заголовок длиннее {} байт", head.len());
            send(socket, http::RESPONSE_BAD_REQUEST.as_bytes()).await;
        }
        Head::Closed => {}
    }
}

/// Собрать plain-ответ и вытолкнуть его клиенту.
async fn reply(
    socket: &mut TcpSocket<'_>,
    response: &mut heapless::String<{ http::RESPONSE_MAX }>,
    status: &str,
    text: &str,
) {
    http::write_plain(response, status, text);
    send(socket, response.as_bytes()).await;
}

/// Принять образ OTA: тело POST-запроса пишется потоком в неактивный Слот;
/// полный образ активируется, после чего Гирлянда перезагружается. Любое
/// отклонение (битый образ, обрыв, ошибка флеша) оставляет otadata нетронутым —
/// прежняя Прошивка продолжает работать.
async fn receive_image(
    socket: &mut TcpSocket<'_>,
    content_length: usize,
    head: &[u8],
    response: &mut heapless::String<{ http::RESPONSE_MAX }>,
    flash: &mut FlashStorage<'static>,
    ota: &mut ota::OtaBuffers,
) {
    info!("OTA: заливка образа, Content-Length: {content_length}");
    match stream_image(socket, content_length, head, flash, ota).await {
        Ok(image_len) => {
            info!("OTA: образ {image_len} байт записан и активирован — перезагрузка");
            reply(socket, response, "200 OK", http::TEXT_OTA_OK).await;
            // Ответ должен успеть уйти клиенту, затем — ребут в новую Прошивку.
            Timer::after(Duration::from_millis(OTA_REBOOT_DELAY_MS)).await;
            software_reset();
        }
        Err(rejection) => {
            // Каждый отказ — до активации: прежняя Прошивка продолжает работать.
            if let Some((status, text)) = rejection.answer() {
                reply(socket, response, status, text).await;
            }
        }
    }
}

/// Записать тело образа в неактивный Слот и активировать его; `Ok` — размер
/// записанного образа, Слот активирован. Отказы — до активации, кроме
/// [`Rejection::Aborted`]: клиента уже нет, отвечать некому.
// Кадр фьючерса живёт в статическом пуле embassy (bss) — в цепочке
// http_task → serve → receive_image, стек при работе не расходуется;
// разрешение согласовано (тот же класс, что у http_task/serve).
#[allow(
    clippy::large_stack_frames,
    reason = "фьючерс живёт в статическом пуле embassy (bss), стек при работе не расходуется"
)]
async fn stream_image(
    socket: &mut TcpSocket<'_>,
    content_length: usize,
    head: &[u8],
    flash: &mut FlashStorage<'static>,
    ota: &mut ota::OtaBuffers,
) -> Result<u32, Rejection> {
    let mut session = ota::Session::begin(flash, &mut *ota.table).map_err(Rejection::Flash)?;
    let mut writer = session.open_writer(ota.sector).map_err(Rejection::Flash)?;
    if content_length == 0 {
        return Err(Rejection::Empty);
    }
    if content_length > writer.capacity() {
        return Err(Rejection::TooLarge {
            size: content_length,
            capacity: writer.capacity(),
        });
    }
    // Часть тела могла прийти вместе с заголовком — она уже в буфере.
    let mut received = 0;
    let mut first_byte: Option<u8> = None;
    let initial = (head.len() - http::body_start(head)).min(content_length);
    if initial > 0 {
        feed(
            &mut writer,
            &head[http::body_start(head)..][..initial],
            &mut first_byte,
        )?;
        received = initial;
    }
    while received < content_length {
        let want = ota.chunk.len().min(content_length - received);
        match socket.read(&mut ota.chunk[..want]).await {
            // Обрыв соединения — образ недописан и не активируется.
            Ok(0) => {
                warn!("OTA: соединение оборвалось на {received}/{content_length} байт");
                return Err(Rejection::Aborted);
            }
            Ok(n) => {
                feed(&mut writer, &ota.chunk[..n], &mut first_byte)?;
                received += n;
            }
            Err(e) => {
                warn!("OTA: ошибка чтения сокета: {e:?} — образ не активируется");
                return Err(Rejection::Aborted);
            }
        }
    }
    // Битый образ отклоняем до активации; образ с верным началом, но битый
    // внутри, отсеет сам загрузчик при следующем старте.
    if first_byte != Some(ota::IMAGE_MAGIC) {
        warn!("OTA: тело не похоже на образ ESP-IDF — активации нет");
        return Err(Rejection::NotAnImage);
    }
    let image_len = writer.finish().map_err(Rejection::Flash)?;
    session.activate().map_err(Rejection::Flash)?;
    Ok(image_len)
}

/// Отказ в заливке образа: все варианты, кроме [`Rejection::Aborted`],
/// получают клиентский ответ и не трогают активный Слот.
enum Rejection {
    /// Клиент закрыл соединение или сокет ошибнулся.
    Aborted,
    /// Тело отсутствует или объявлено пустым.
    Empty,
    /// Тело не похоже на образ ESP-IDF.
    NotAnImage,
    /// Образ не помещается в свободный Слот.
    TooLarge {
        /// Размер образа, байт.
        size: usize,
        /// Ёмкость Слота, байт.
        capacity: usize,
    },
    /// Ошибка флеша: таблица разделов, Слот или запись образа.
    Flash(PartitionError),
}

impl Rejection {
    /// Ответ клиенту: статус и тело; `None` — отвечать некому.
    fn answer(&self) -> Option<(&'static str, &'static str)> {
        match self {
            Rejection::Aborted => None,
            Rejection::Empty => Some(("400 Bad Request", "")),
            Rejection::NotAnImage => Some(("400 Bad Request", http::TEXT_BAD_IMAGE)),
            Rejection::TooLarge { size, capacity } => {
                warn!("OTA: образ {size} байт не помещается в Слот {capacity} байт");
                Some(("413 Payload Too Large", http::TEXT_PAYLOAD_TOO_LARGE))
            }
            Rejection::Flash(error) => {
                warn!("OTA: флеш: {error:?} — активации нет");
                Some(("500 Internal Server Error", http::TEXT_INTERNAL_ERROR))
            }
        }
    }
}

/// Подложить чанк тела в писатель, попутно запомнив первый байт образа.
fn feed(
    writer: &mut ota::ImageWriter<'_, ota::RegionTarget<'_, '_>>,
    chunk: &[u8],
    first_byte: &mut Option<u8>,
) -> Result<(), Rejection> {
    if first_byte.is_none() {
        *first_byte = chunk.first().copied();
    }
    match writer.extend(chunk) {
        Ok(()) => Ok(()),
        // Content-Length ≤ ёмкости Слота проверен до первой записи,
        // переполнение недостижимо.
        Err(ota::ExtendError::Overflow) => unreachable!("guard ёмкости образа"),
        Err(ota::ExtendError::Target(error)) => {
            warn!("OTA: запись в Слот не удалась: {error:?} — активации нет");
            Err(Rejection::Flash(error))
        }
    }
}

/// Итог чтения заголовка из сокета.
enum Head {
    /// В буфере есть конец заголовка; столько байт реально прочитано —
    /// хвост буфера за этим числом может содержать мусор прошлых запросов.
    Complete(usize),
    /// Буфер заполнен без конца заголовка.
    Overflow,
    /// Клиент закрыл соединение или ошибка чтения.
    Closed,
}

/// Читает из сокета до конца заголовка или заполнения буфера. Сам буфер
/// остаётся у вызывающего — [`http::parse_request`] читает его оттуда.
async fn read_head(socket: &mut TcpSocket<'_>, buf: &mut [u8]) -> Head {
    let mut n = 0;
    while n < buf.len() {
        match socket.read(&mut buf[n..]).await {
            Ok(0) => return Head::Closed,
            Ok(read) => {
                n += read;
                if http::head_complete(&buf[..n]) {
                    return Head::Complete(n);
                }
            }
            Err(e) => {
                warn!("HTTP: ошибка чтения: {e:?}");
                return Head::Closed;
            }
        }
    }
    Head::Overflow
}

/// Дописать ответ в сокет целиком и вытолкнуть; ошибку пишем в журнал —
/// отвалившийся клиент не должен ронять сервер.
async fn send(socket: &mut TcpSocket<'_>, mut data: &[u8]) {
    while !data.is_empty() {
        match socket.write(data).await {
            Ok(0) => break,
            Ok(written) => data = &data[written..],
            Err(e) => {
                warn!("HTTP: ошибка записи: {e:?}");
                return;
            }
        }
    }
    if let Err(e) = socket.flush().await {
        warn!("HTTP: ошибка вытолкнения: {e:?}");
    }
}
