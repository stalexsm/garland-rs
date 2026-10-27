//! Мини-HTTP: разбор запросов и сборка ответов страницы версии Прошивки.
//!
//! Чистые функции без сети: на входе — байты из TCP-сокета, на выходе —
//! разобранный запрос или готовый ответ. Так разбор и ответы проверяются
//! хост-тестами (`cargo test`), а `net.rs` остаётся чистой обвязкой сокета.

/// Разобранный запрос: только то, что нужно Прошивке сейчас.
#[derive(Debug, PartialEq, Eq)]
pub enum Request<'a> {
    /// `GET /` — страница версии Прошивки.
    GetRoot,
    /// `POST /ota` — заливка образа OTA; длина тела объявлена Content-Length,
    /// контрольная сумма образа — `X-OTA-SHA256`.
    PostOta {
        /// Объявленный размер тела, байт.
        content_length: usize,
        /// Ожидаемая клиентом сумма SHA256 тела (32 байта из hex-заголовка).
        digest: [u8; 32],
    },
    /// Корректный HTTP-запрос с другим методом или путём.
    Other {
        /// Метод запроса (`GET`, `POST`, …).
        method: &'a str,
        /// Путь запроса (`/`, `/ota`, …).
        path: &'a str,
    },
    /// `POST /ota` с чужого Origin: похоже на CSRF — страница из интернета
    /// пытается вслепую прошить Гирлянду.
    CrossOrigin,
    /// Начало потока не разбирается как HTTP-запрос.
    Malformed,
}

/// Вместимость буфера ответа страницы: сейчас страница занимает ~1,6 КБ,
/// ёмкость взята с запасом.
pub const RESPONSE_MAX: usize = 3072;

/// Заголовок ответа до значения Content-Length.
const RESPONSE_HEAD: &str = "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\nContent-Length: ";

/// Конец заголовка ответа.
const RESPONSE_HEAD_END: &str = "\r\n\r\n";

/// Конец заголовка запроса и ответа.
const HEAD_END: &[u8; 4] = b"\r\n\r\n";

/// Начало HTML-тела страницы (до версии Прошивки).
const PAGE_OPEN: &str = "<!doctype html><html><head><meta charset=\"utf-8\"><title>Гирлянда</title></head><body><h1>Гирлянда</h1><p>Версия Прошивки: ";

/// Форма заливки OTA: файл уходит целиком методом fetch, пароль и сумма
/// SHA256 (считается в браузере) — заголовками `X-OTA-Pass` и `X-OTA-SHA256`.
const PAGE_FORM_OTA: &str = "</p><h2>OTA-обновление</h2><p>Выберите файл образа (espflash save-image, .bin) и введите пароль OTA (переменная сборки OTA_PASS): Гирлянда сверит SHA256, запишет образ в свободный Слот и перезагрузится. Во время заливки не отключайте питание.</p><form onsubmit=\"ota(event)\"><p><input type=\"file\" accept=\".bin\" required> <input type=\"password\" id=\"ota-pass\" placeholder=\"Пароль OTA\" required> <button>Залить и перезагрузить</button></p></form><p id=\"ota-status\"></p><script>async function ota(e){e.preventDefault();const s=document.getElementById('ota-status');const file=e.target.querySelector('input[type=file]').files[0];const pass=document.getElementById('ota-pass').value;s.textContent='Считаю SHA-256…';try{const hex=[...new Uint8Array(await crypto.subtle.digest('SHA-256',await file.arrayBuffer()))].map(b=>b.toString(16).padStart(2,'0')).join('');s.textContent='Заливаю…';const r=await fetch('/ota',{method:'POST',body:file,headers:{'X-OTA-Pass':pass,'X-OTA-SHA256':hex}});if(r.ok){s.textContent='Принято — Гирлянда перезагружается…';setTimeout(()=>location.reload(),15000)}else{s.textContent='Ошибка '+r.status}}catch(_){s.textContent='Обрыв соединения — образ не принят'}}</script></body></html>";

/// Хвост страницы при отключённой заливке: вместо формы — пояснение.
const PAGE_FORM_LOCKED: &str = "</p><p>OTA-обновление отключено: пароль заливки (OTA_PASS) не задан при сборке.</p></body></html>";

/// Записать в буфер HTTP-ответ 200 со страницей Гирлянды: заголовки с верным
/// Content-Length и тело с версией Прошивки и формой заливки OTA. Буфер
/// очищается; сборка push_str'ом, без core::fmt — кадр задачи HTTP-сервера
/// остаётся маленьким.
pub fn write_page(page: &mut heapless::String<RESPONSE_MAX>, version: &str, ota_enabled: bool) {
    let form = if ota_enabled {
        PAGE_FORM_OTA
    } else {
        PAGE_FORM_LOCKED
    };
    let body_len = PAGE_OPEN.len() + version.len() + form.len();
    page.clear();
    put(page, RESPONSE_HEAD);
    push_usize(page, body_len);
    put(page, RESPONSE_HEAD_END);
    put(page, PAGE_OPEN);
    put(page, version);
    put(page, form);
}

/// Дописать строку: буфер рассчитан на страницу целиком, переполнение —
/// ошибка программиста, паника ловит её на месте.
fn put(page: &mut heapless::String<RESPONSE_MAX>, text: &str) {
    page.push_str(text).expect("страница помещается в буфер");
}

/// Дописать число в десятичной записи. Длина — всегда в пределах usize,
/// буфер всегда достаточен: паника ловит ошибку программиста, не данные.
fn push_usize(page: &mut heapless::String<RESPONSE_MAX>, mut value: usize) {
    let mut digits = [0u8; 20]; // 20 — длина u64::MAX в десятичной записи
    let mut count = 0;
    loop {
        digits[count] = b'0' + (value % 10) as u8;
        value /= 10;
        count += 1;
        if value == 0 {
            break;
        }
    }
    for &digit in digits[..count].iter().rev() {
        page.push(digit as char)
            .expect("страница помещается в буфер");
    }
}

/// Ответ для корректных, но неподдерживаемых запросов (чужой путь или метод).
pub const RESPONSE_NOT_FOUND: &str =
    "HTTP/1.1 404 Not Found\r\nConnection: close\r\nContent-Length: 0\r\n\r\n";

/// Ответ на поток, не разбирающийся как HTTP-запрос.
pub const RESPONSE_BAD_REQUEST: &str =
    "HTTP/1.1 400 Bad Request\r\nConnection: close\r\nContent-Length: 0\r\n\r\n";

/// Ответ на корректный запрос с чужого Origin (похоже на CSRF).
pub const RESPONSE_FORBIDDEN: &str =
    "HTTP/1.1 403 Forbidden\r\nConnection: close\r\nContent-Length: 0\r\n\r\n";

/// Начало plain-ответа до статуса (собирается [`write_plain`] — длины тела
/// считаются, ручных Content-Length нет).
const PLAIN_STATUS_HEAD: &str = "HTTP/1.1 ";

/// Заголовки plain-ответа после статуса, до длины тела.
const PLAIN_HEAD_MID: &str =
    "\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\nContent-Length: ";

/// Конец заголовков plain-ответа.
const PLAIN_HEAD_END: &str = "\r\n\r\n";

/// Тело ответа на завершённую заливку: образ записан и активирован, дальше —
/// перезагрузка.
pub const TEXT_OTA_OK: &str = "Образ принят, Гирлянда перезагружается\n";

/// Тело ответа на битый образ (не похож на ESP-IDF app image): Слот остаётся
/// неактивным, прежняя Прошивка продолжает работать.
pub const TEXT_BAD_IMAGE: &str = "Файл не похож на образ прошивки (0xE9)\n";

/// Тело ответа, когда клиент не назвал пароль заливки или назвал неверный:
/// Слот не тронут.
pub const TEXT_OTA_UNAUTHORIZED: &str = "Неверный пароль заливки (заголовок X-OTA-Pass)\n";

/// Тело ответа, когда заливка отключена: пароль заливки не задан при сборке.
pub const TEXT_OTA_LOCKED: &str =
    "Заливка отключена: пароль заливки (OTA_PASS) не задан при сборке\n";

/// Тело ответа, когда сумма SHA256 не совпала: активации нет.
pub const TEXT_BAD_DIGEST: &str = "Контрольная сумма SHA256 не сошлась — образ не активирован\n";

/// Тело ответа, когда образ не помещается в свободный Слот.
pub const TEXT_PAYLOAD_TOO_LARGE: &str = "Образ не помещается в свободный Слот\n";

/// Тело ответа на ошибку флеша при заливке: обновление не активировано.
pub const TEXT_INTERNAL_ERROR: &str = "Запись во флеш не удалась, обновление отменено\n";

/// Записать в буфер plain-ответ с заданным статусом и телом: заголовки с
/// верным Content-Length (вычисляется по телу). Буфер очищается; сборка
/// push_str'ом, без core::fmt — кадр задачи HTTP-сервера остаётся маленьким.
pub fn write_plain(page: &mut heapless::String<RESPONSE_MAX>, status: &str, text: &str) {
    page.clear();
    put(page, PLAIN_STATUS_HEAD);
    put(page, status);
    put(page, PLAIN_HEAD_MID);
    push_usize(page, text.len());
    put(page, PLAIN_HEAD_END);
    put(page, text);
}

/// Разобрать начало запроса (до `\r\n\r\n`): `None` — заголовок ещё
/// неполный, ждать продолжения из сокета. `known_hosts` — имена Гирлянды
/// (DHCP-имя и адрес): Origin заливки обязан быть одним из них, сверка
/// с Host из запроса обходится DNS-rebinding'ом.
pub fn parse_request<'a>(buf: &'a [u8], known_hosts: &[&str]) -> Option<Request<'a>> {
    let head_end = find_head_end(buf)?;
    let head = &buf[..head_end];
    let line_end = head.iter().position(|&b| b == b'\n').unwrap_or(head.len());
    let line = core::str::from_utf8(&head[..line_end]).unwrap_or("");
    let mut parts = line.split_ascii_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("");
    let version = parts.next().unwrap_or("");
    if !version.starts_with("HTTP/") {
        return Some(Request::Malformed);
    }
    route(method, path, head, known_hosts)
}

/// Маршрут по методу и пути; отдельная функция, чтобы кадры разбора строки
/// запроса и поиска Content-Length не складывались в один выше порога
/// large_stack_frames.
fn route<'a>(
    method: &'a str,
    path: &'a str,
    head: &'a [u8],
    known_hosts: &[&str],
) -> Option<Request<'a>> {
    Some(match (method, path) {
        ("GET", "/") => Request::GetRoot,
        ("POST", "/ota") => {
            if cross_origin(head, known_hosts) {
                Request::CrossOrigin
            } else {
                match (content_length(head), digest_header(head)) {
                    (Some(content_length), Some(digest)) => Request::PostOta {
                        content_length,
                        digest,
                    },
                    _ => Request::Malformed,
                }
            }
        }
        _ => Request::Other { method, path },
    })
}

/// Значение заголовка по имени (регистр букв не важен); `None` — заголовка
/// нет.
fn header_value<'a>(head: &'a [u8], name: &[u8]) -> Option<&'a [u8]> {
    // Сканирование по индексам, без итератора split: кадр разбора запроса
    // держится ниже порога large_stack_frames (канон find_head_end).
    // Последняя строка может быть без \n (хвост перед \r\n\r\n) — тоже строка.
    let mut line_start = 0;
    loop {
        let line = match head[line_start..].iter().position(|&b| b == b'\n') {
            Some(nl) => {
                let end = line_start + nl;
                let line = &head[line_start..end];
                line_start = end + 1;
                line
            }
            // Последняя строка без \n: сдвигаемся до конца, иначе тот же
            // хвост обработается снова — бесконечный цикл.
            None if line_start < head.len() => {
                let line = &head[line_start..];
                line_start = head.len();
                line
            }
            None => return None,
        };
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        let Some(colon) = line.iter().position(|&b| b == b':') else {
            continue;
        };
        if !line[..colon].eq_ignore_ascii_case(name) {
            continue;
        }
        // Пробелы вокруг значения (HTTP OWS) — не часть значения.
        return Some(line[colon + 1..].trim_ascii());
    }
}

/// Значение заголовка Content-Length; `None` — заголовка нет или он не число.
fn content_length(head: &[u8]) -> Option<usize> {
    let value = core::str::from_utf8(header_value(head, b"Content-Length")?).ok()?;
    decimal_usize(value.as_bytes())
}

/// Чужая ли страница шлёт заливку (CSRF): у страницы Гирлянды Origin —
/// одно из `known_hosts`. Host из запроса не сверяется: при DNS-rebinding
/// он совпадает с Origin атакующей страницы. curl Origin не шлёт — CSRF
/// на него не рассчитан.
fn cross_origin(head: &[u8], known_hosts: &[&str]) -> bool {
    let Some(origin) = header_value(head, b"Origin") else {
        return false;
    };
    let Ok(origin) = core::str::from_utf8(origin) else {
        return true;
    };
    let Some(origin_host) = origin.strip_prefix("http://") else {
        // Страница Гирлянды живёт на http; иная схема — не она.
        return true;
    };
    !known_hosts.contains(&origin_host)
}

/// Значение заголовка `X-OTA-Pass`; `None` — заголовка нет или он не текст.
pub fn ota_password(head: &[u8]) -> Option<&str> {
    core::str::from_utf8(header_value(head, b"X-OTA-Pass")?).ok()
}

/// Вердикт проверки доступа к заливке OTA — до чтения тела.
#[derive(Debug, PartialEq, Eq)]
pub enum OtaAccess {
    /// Пароль совпал: заливку можно начинать.
    Granted,
    /// Заливка отключена: пароль не задан при сборке (пустой `OTA_PASS`).
    Locked,
    /// Клиент не назвал пароль или назвал неверный.
    Denied,
}

/// Сверить пароль заливки: `configured` — пароль из сборки (пустой
/// отключает заливку целиком), `provided` — значение `X-OTA-Pass` клиента.
pub fn authorize_ota(configured: &str, provided: Option<&str>) -> OtaAccess {
    if configured.is_empty() {
        OtaAccess::Locked
    } else if provided == Some(configured) {
        OtaAccess::Granted
    } else {
        OtaAccess::Denied
    }
}

/// Значение заголовка `X-OTA-SHA256`: 64-знаковый hex → 32 байта; заголовка
/// нет, длина не та или мусор — `None` (запрос без суммы не заливается).
fn digest_header(head: &[u8]) -> Option<[u8; 32]> {
    hex_digest(header_value(head, b"X-OTA-SHA256")?)
}

/// 64-знаковая шестнадцатеричная строка → 32 байта; мусор или длина не та —
/// `None`.
fn hex_digest(text: &[u8]) -> Option<[u8; 32]> {
    if text.len() != 64 {
        return None;
    }
    let mut digest = [0u8; 32];
    for (i, pair) in text.as_chunks::<2>().0.iter().enumerate() {
        let hi = (pair[0] as char).to_digit(16)?;
        let lo = (pair[1] as char).to_digit(16)?;
        digest[i] = (hi * 16 + lo) as u8;
    }
    Some(digest)
}

/// Десятичное число без знака; мусор или переполнение — `None`.
/// Ручное накопление вместо `str::parse`: инлайн from_str_radix раздувает
/// кадр parse_request выше порога large_stack_frames.
fn decimal_usize(text: &[u8]) -> Option<usize> {
    if text.is_empty() {
        return None;
    }
    let mut value = 0usize;
    for &b in text {
        if !b.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add((b - b'0') as usize)?;
    }
    Some(value)
}

/// Есть ли в буфере конец заголовка (`\r\n\r\n`): можно разбирать запрос.
pub fn head_complete(buf: &[u8]) -> bool {
    find_head_end(buf).is_some()
}

/// Индекс первого байта тела после конца заголовка; вызывать после
/// [`head_complete`] — конец заголовка обязан быть в буфере.
pub fn body_start(buf: &[u8]) -> usize {
    find_head_end(buf).expect("конец заголовка") + HEAD_END.len()
}

/// Первый индекс `\r\n\r\n` в буфере, если он там есть.
fn find_head_end(buf: &[u8]) -> Option<usize> {
    if buf.len() < HEAD_END.len() {
        return None;
    }
    (0..=buf.len() - HEAD_END.len()).find(|&i| &buf[i..i + HEAD_END.len()] == HEAD_END)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Сумма SHA256 в тестах: hex «00..1f».
    const HEX32: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";

    /// Байты [`HEX32`].
    const BYTES32: [u8; 32] = [
        0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
        0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d,
        0x1e, 0x1f,
    ];

    #[test]
    fn get_root_recognized() {
        let bytes = b"GET / HTTP/1.1\r\nHost: garland\r\nAccept: text/html\r\n\r\n";
        assert_eq!(parse_request(bytes, &[]), Some(Request::GetRoot));
    }

    #[test]
    fn incomplete_head_is_none() {
        let bytes = b"GET / HTTP/1.1\r\nHost: garland\r\nAcc";
        assert_eq!(parse_request(bytes, &[]), None);
    }

    #[test]
    fn head_end_detected_by_bytes() {
        assert!(!head_complete(b"GET / HTTP/1.1\r\nHost: ga"));
        assert!(!head_complete(b"\r\n\r"));
        assert!(head_complete(b"GET / HTTP/1.1\r\n\r\n"));
        // Конец заголовка приходит частями — детект по байтам, не по строкам.
        assert!(head_complete(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n"));
    }

    #[test]
    fn other_method_and_path_reported() {
        assert_eq!(
            parse_request(b"GET /ota HTTP/1.1\r\n\r\n", &[]),
            Some(Request::Other {
                method: "GET",
                path: "/ota"
            })
        );
        assert_eq!(
            parse_request(b"POST / HTTP/1.1\r\nContent-Length: 5\r\n\r\nhello", &[]),
            Some(Request::Other {
                method: "POST",
                path: "/"
            })
        );
    }

    #[test]
    fn post_ota_recognized_with_content_length_and_digest() {
        let bytes = format!(
            "POST /ota HTTP/1.1\r\nHost: g\r\nContent-Length: 1048576\r\nX-OTA-SHA256: {HEX32}\r\n\r\n"
        );
        assert_eq!(
            parse_request(bytes.as_bytes(), &[]),
            Some(Request::PostOta {
                content_length: 1_048_576,
                digest: BYTES32
            })
        );
        // Браузеры шлют каноничное имя, но HTTP-заголовки регистронезависимы.
        let mixed =
            format!("POST /ota HTTP/1.1\r\ncontent-length: 12\r\nx-ota-sha256: {HEX32}\r\n\r\n");
        assert_eq!(
            parse_request(mixed.as_bytes(), &[]),
            Some(Request::PostOta {
                content_length: 12,
                digest: BYTES32
            })
        );
        // Часть тела может прийти вместе с заголовком — разбору не мешает.
        let mut body =
            format!("POST /ota HTTP/1.1\r\nContent-Length: 4\r\nX-OTA-SHA256: {HEX32}\r\n\r\n")
                .into_bytes();
        body.extend_from_slice(b"\xe9\x00\x02\x00");
        assert_eq!(
            parse_request(&body, &[]),
            Some(Request::PostOta {
                content_length: 4,
                digest: BYTES32
            })
        );
    }

    #[test]
    fn post_ota_without_length_or_digest_is_malformed() {
        // Нет Content-Length.
        let no_length = format!("POST /ota HTTP/1.1\r\nHost: g\r\nX-OTA-SHA256: {HEX32}\r\n\r\n");
        assert_eq!(
            parse_request(no_length.as_bytes(), &[]),
            Some(Request::Malformed)
        );
        // Content-Length — не число.
        let bad_length =
            format!("POST /ota HTTP/1.1\r\nContent-Length: abc\r\nX-OTA-SHA256: {HEX32}\r\n\r\n");
        assert_eq!(
            parse_request(bad_length.as_bytes(), &[]),
            Some(Request::Malformed)
        );
        // Нет суммы или она не 64-знаковый hex.
        assert_eq!(
            parse_request(b"POST /ota HTTP/1.1\r\nContent-Length: 4\r\n\r\n", &[]),
            Some(Request::Malformed)
        );
        assert_eq!(
            parse_request(
                b"POST /ota HTTP/1.1\r\nContent-Length: 4\r\nX-OTA-SHA256: xyz\r\n\r\n",
                &[]
            ),
            Some(Request::Malformed)
        );
    }

    #[test]
    fn post_ota_cross_origin_is_rejected() {
        let known = &["192.168.4.1"];
        // CSRF: страница из интернета пытается прошить Гирлянду вслепую.
        assert_eq!(
            parse_request(
                b"POST /ota HTTP/1.1\r\nHost: 192.168.4.1\r\nOrigin: http://evil.example\r\nContent-Length: 4\r\n\r\n",
                known
            ),
            Some(Request::CrossOrigin)
        );
        // Origin с тем же адресом, но другим портом — тоже чужой.
        assert_eq!(
            parse_request(
                b"POST /ota HTTP/1.1\r\nHost: 192.168.4.1\r\nOrigin: http://192.168.4.1:8080\r\nContent-Length: 4\r\n\r\n",
                known
            ),
            Some(Request::CrossOrigin)
        );
        // Origin без Host — отклоняем: обычный браузер Host всегда шлёт.
        assert_eq!(
            parse_request(
                b"POST /ota HTTP/1.1\r\nOrigin: http://g\r\nContent-Length: 4\r\n\r\n",
                known
            ),
            Some(Request::CrossOrigin)
        );
    }

    /// Rebinding: пара Origin/Host совпадает, но Origin не из known_hosts —
    /// отказ.
    #[test]
    fn rebinding_with_matching_host_is_rejected() {
        let known = &["garland", "192.168.4.1"];
        let rebinding = format!(
            "POST /ota HTTP/1.1\r\nHost: evil.example\r\nOrigin: http://evil.example\r\nContent-Length: 4\r\nX-OTA-SHA256: {HEX32}\r\n\r\n"
        );
        assert_eq!(
            parse_request(rebinding.as_bytes(), known),
            Some(Request::CrossOrigin)
        );
        // Та же пара, но с известным именем — своя страница проходит разбор.
        let own = format!(
            "POST /ota HTTP/1.1\r\nHost: 192.168.4.1\r\nOrigin: http://192.168.4.1\r\nContent-Length: 4\r\nX-OTA-SHA256: {HEX32}\r\n\r\n"
        );
        assert_eq!(
            parse_request(own.as_bytes(), known),
            Some(Request::PostOta {
                content_length: 4,
                digest: BYTES32
            })
        );
    }

    #[test]
    fn post_ota_same_origin_and_curl_are_accepted() {
        // Страница самой Гирлянды: Origin — известное имя устройства.
        let own = format!(
            "POST /ota HTTP/1.1\r\nHost: 192.168.4.1\r\nOrigin: http://192.168.4.1\r\nContent-Length: 4\r\nX-OTA-SHA256: {HEX32}\r\n\r\n"
        );
        assert_eq!(
            parse_request(own.as_bytes(), &["192.168.4.1"]),
            Some(Request::PostOta {
                content_length: 4,
                digest: BYTES32
            })
        );
        // curl Origin не шлёт — не браузер, CSRF невозможен.
        let curl = format!(
            "POST /ota HTTP/1.1\r\nHost: g\r\nContent-Length: 4\r\nX-OTA-SHA256: {HEX32}\r\n\r\n"
        );
        assert_eq!(
            parse_request(curl.as_bytes(), &[]),
            Some(Request::PostOta {
                content_length: 4,
                digest: BYTES32
            })
        );
    }

    /// Доступ к заливке решается до чтения тела: пустой пароль сборки —
    /// заливка отключена, неверный или неназванный — отказ.
    #[test]
    fn ota_access_decides_before_body() {
        assert_eq!(authorize_ota("", None), OtaAccess::Locked);
        assert_eq!(authorize_ota("", Some("пароль")), OtaAccess::Locked);
        assert_eq!(authorize_ota("s3cret", None), OtaAccess::Denied);
        assert_eq!(authorize_ota("s3cret", Some("wrong")), OtaAccess::Denied);
        assert_eq!(authorize_ota("s3cret", Some("s3cret")), OtaAccess::Granted);
    }

    /// Пароль заливки читается из заголовка независимо от регистра имени.
    #[test]
    fn ota_password_header_is_read() {
        assert_eq!(
            ota_password(b"POST /ota HTTP/1.1\r\nX-OTA-Pass: s3cret\r\n\r\n"),
            Some("s3cret")
        );
        assert_eq!(
            ota_password(b"POST /ota HTTP/1.1\r\nx-ota-pass: s3cret\r\n\r\n"),
            Some("s3cret")
        );
        assert_eq!(ota_password(b"POST /ota HTTP/1.1\r\n\r\n"), None);
    }

    #[test]
    fn body_start_points_past_head() {
        let bytes = b"POST /ota HTTP/1.1\r\nContent-Length: 4\r\n\r\n\xe9\x00";
        assert!(head_complete(bytes));
        assert_eq!(&bytes[body_start(bytes)..], b"\xe9\x00");
    }

    #[test]
    fn garbage_is_malformed() {
        // Непохоже на request-line, но с валидным концом заголовка.
        assert_eq!(
            parse_request(b"\x00\xff\r\n\r\n", &[]),
            Some(Request::Malformed)
        );
        assert_eq!(parse_request(b"GET\r\n\r\n", &[]), Some(Request::Malformed));
    }

    #[test]
    fn version_page_is_valid_response() {
        let mut page = heapless::String::<RESPONSE_MAX>::new();
        write_page(&mut page, "1.2.3", true);
        let response = page.as_str();
        assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(response.contains("Content-Type: text/html; charset=utf-8"));
        assert!(response.contains("Connection: close\r\n"));
        let (head, body) = response.split_once("\r\n\r\n").expect("конец заголовка");
        let length = head
            .lines()
            .find_map(|l| l.strip_prefix("Content-Length: "))
            .expect("Content-Length");
        assert_eq!(length, body.len().to_string());
        assert!(body.contains("Версия Прошивки: 1.2.3"));
        // Форма заливки: пароль и сумма SHA256 уходят заголовками fetch.
        assert!(body.contains("type=\"file\""));
        assert!(body.contains("type=\"password\""));
        assert!(body.contains("'X-OTA-Pass'"));
        assert!(body.contains("'X-OTA-SHA256'"));
    }

    /// Без пароля сборки страница заливки не показывает: форма заменена
    /// пояснением.
    #[test]
    fn locked_page_hides_ota_form() {
        let mut page = heapless::String::<RESPONSE_MAX>::new();
        write_page(&mut page, "1.2.3", false);
        let body = page.as_str();
        assert!(body.contains("Версия Прошивки: 1.2.3"));
        assert!(body.contains("отключено"));
        assert!(!body.contains("<form"));
    }

    #[test]
    fn plain_answers_are_well_formed() {
        for (status, text) in [
            ("200 OK", TEXT_OTA_OK),
            ("400 Bad Request", TEXT_BAD_IMAGE),
            ("401 Unauthorized", TEXT_OTA_UNAUTHORIZED),
            ("403 Forbidden", TEXT_OTA_LOCKED),
            ("413 Payload Too Large", TEXT_PAYLOAD_TOO_LARGE),
            ("422 Unprocessable Entity", TEXT_BAD_DIGEST),
            ("500 Internal Server Error", TEXT_INTERNAL_ERROR),
        ] {
            let mut page = heapless::String::<RESPONSE_MAX>::new();
            write_plain(&mut page, status, text);
            let response = page.as_str();
            assert!(response.starts_with(&format!("HTTP/1.1 {status}\r\n")));
            assert!(response.contains("Content-Type: text/plain; charset=utf-8\r\n"));
            assert!(response.contains("Connection: close\r\n"));
            let (head, body) = response.split_once("\r\n\r\n").expect("конец заголовка");
            let length = head
                .lines()
                .find_map(|l| l.strip_prefix("Content-Length: "))
                .expect("Content-Length");
            assert_eq!(length, body.len().to_string());
            assert_eq!(body, text);
        }
    }

    #[test]
    fn rejects_are_well_formed() {
        for response in [RESPONSE_NOT_FOUND, RESPONSE_BAD_REQUEST, RESPONSE_FORBIDDEN] {
            let (head, body) = response.split_once("\r\n\r\n").expect("конец заголовка");
            let length = head
                .lines()
                .find_map(|l| l.strip_prefix("Content-Length: "))
                .expect("Content-Length");
            assert_eq!(length, body.len().to_string());
        }
        assert!(RESPONSE_NOT_FOUND.starts_with("HTTP/1.1 404 Not Found\r\n"));
        assert!(RESPONSE_BAD_REQUEST.starts_with("HTTP/1.1 400 Bad Request\r\n"));
        assert!(RESPONSE_FORBIDDEN.starts_with("HTTP/1.1 403 Forbidden\r\n"));
    }
}
