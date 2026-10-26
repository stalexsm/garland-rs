//! Сеть Гирлянды: Wi-Fi STA (esp-radio), стек embassy-net с DHCP и именем
//! `garland`, HTTP-сервер страницы версии Прошивки.
//!
//! Каждая часть — отдельная embassy-задача, не зависящая от задачи Ленты:
//! сбой или недоступность сети кончаются повторной попыткой и не касаются
//! рендера (эффекты идут при любой сети).

use crate::http;
use crate::{WIFI_PASS, WIFI_SSID};
use alloc::boxed::Box;
use embassy_executor::Spawner;
use embassy_net::tcp::TcpSocket;
use embassy_net::{DhcpConfig, Runner, Stack, StackResources};
use embassy_time::{Duration, Timer};
use esp_radio::wifi::{
    AuthenticationMethodConfig, Config, ControllerConfig, Interface, WifiController,
    sta::StationConfig,
};
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

/// Размер буфера заголовка запроса, байт: страница версии не требует длинных
/// заголовков, переполнение отвечает 400.
const HEAD_BUF_SIZE: usize = 1024;

/// Размеры TCP-буферов сокета, байт: заголовок и страница — сотни байт.
const SOCKET_RX_SIZE: usize = 1024;
const SOCKET_TX_SIZE: usize = 1024;

/// Слоты сокетов стека: DHCP + TCP-сервер (канон примера esp-hal — 3).
const STACK_SOCKETS: usize = 3;

/// Поднять сеть: контроллер Wi-Fi со сборочными учётными данными, стек
/// embassy-net (DHCP + [`HOSTNAME`]) и задачи — коннектор, драйвер стека и
/// HTTP-сервер. Вызывается из `main`, только когда учётные данные заданы.
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
    spawner.spawn(http_task(stack, version).expect("задача HTTP-сервера не поднялась"));
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
// large_stack_frames меряет фьючерс embassy-задачи (буферы сокета и страницы);
// embassy выделяет его статически (bss), стек при работе не расходуется.
#[allow(
    clippy::large_stack_frames,
    reason = "фьючерс embassy-задачи выделяется статически (bss), стек не расходуется"
)]
#[embassy_executor::task]
async fn http_task(stack: Stack<'static>, version: &'static str) {
    stack.wait_config_up().await;
    info!("HTTP: адрес получен, сервер на порту {HTTP_PORT}");
    // Буферы сокета — в куче один раз: сокет пересоздаётся на каждое
    // соединение, буферы живут столько же, сколько задача.
    let rx: &'static mut [u8; SOCKET_RX_SIZE] = Box::leak(Box::new([0; SOCKET_RX_SIZE]));
    let tx: &'static mut [u8; SOCKET_TX_SIZE] = Box::leak(Box::new([0; SOCKET_TX_SIZE]));
    // Буферы запроса и ответа живут во фьючерсе задачи (bss), а не на стеке.
    let mut head = [0u8; HEAD_BUF_SIZE];
    let mut response = heapless::String::<{ http::RESPONSE_MAX }>::new();
    loop {
        let mut socket = TcpSocket::new(stack, rx, tx);
        socket.set_timeout(Some(Duration::from_millis(TCP_TIMEOUT_MS)));
        if let Err(e) = socket.accept(HTTP_PORT).await {
            warn!("HTTP: accept не удался: {e:?}");
            Timer::after(Duration::from_millis(ACCEPT_RETRY_MS)).await;
            continue;
        }
        serve(&mut socket, version, &mut head, &mut response).await;
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
) {
    match read_head(socket, head).await {
        Head::Complete => match http::parse_request(head) {
            Some(http::Request::GetRoot) => {
                http::write_version_page(response, version);
                send(socket, response.as_bytes()).await;
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

/// Итог чтения заголовка из сокета.
enum Head {
    /// В буфере есть конец заголовка.
    Complete,
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
                    return Head::Complete;
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
