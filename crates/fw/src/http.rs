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
    /// `POST /ota` — заливка образа OTA; длина тела объявлена Content-Length.
    PostOta {
        /// Объявленный размер тела, байт.
        content_length: usize,
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

/// Вместимость буфера ответа страницы: заголовки + тело с версией и
/// формой заливки со скриптом (около 1,3 КБ).
pub const RESPONSE_MAX: usize = 2048;

/// Заголовок ответа до значения Content-Length.
const RESPONSE_HEAD: &str = "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\nContent-Length: ";

/// Конец заголовка ответа.
const RESPONSE_HEAD_END: &str = "\r\n\r\n";

/// Конец заголовка запроса и ответа.
const HEAD_END: &[u8; 4] = b"\r\n\r\n";

/// Начало HTML-тела страницы (до версии Прошивки).
const PAGE_OPEN: &str = "<!doctype html><html><head><meta charset=\"utf-8\"><title>Гирлянда</title></head><body><h1>Гирлянда</h1><p>Версия Прошивки: ";

/// Форма заливки образа OTA после версии: файл уходит на `POST /ota` целиком
/// (без multipart) методом fetch — устройству достаточно читать тело потоком.
const PAGE_FORM: &str = "</p><h2>OTA-обновление</h2><p>Выберите файл образа (espflash save-image, .bin) — Гирлянда запишет его в свободный Слот и перезагрузится. Во время заливки не отключайте питание.</p><form onsubmit=\"return ota(this)\"><p><input type=\"file\" accept=\".bin\" required><button>Залить и перезагрузить</button></p></form><p id=\"ota-status\"></p><script>function ota(f){const s=document.getElementById('ota-status');const file=f.querySelector('input[type=file]').files[0];s.textContent='Заливаю…';fetch('/ota',{method:'POST',body:file}).then(r=>{if(r.ok){s.textContent='Принято — Гирлянда перезагружается…';setTimeout(()=>location.reload(),15000)}else{s.textContent='Ошибка '+r.status}}).catch(()=>s.textContent='Обрыв соединения — образ не принят');return false}</script></body></html>";

/// Записать в буфер HTTP-ответ 200 со страницей Гирлянды: заголовки с верным
/// Content-Length и тело с версией Прошивки и формой заливки OTA. Буфер
/// очищается; сборка push_str'ом, без core::fmt — кадр задачи HTTP-сервера
/// остаётся маленьким.
pub fn write_page(page: &mut heapless::String<RESPONSE_MAX>, version: &str) {
    let body_len = PAGE_OPEN.len() + version.len() + PAGE_FORM.len();
    page.clear();
    put(page, RESPONSE_HEAD);
    push_usize(page, body_len);
    put(page, RESPONSE_HEAD_END);
    put(page, PAGE_OPEN);
    put(page, version);
    put(page, PAGE_FORM);
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

/// Разобрать начало запроса (до `\r\n\r\n`): `None` — заголовок ещё неполный,
/// ждать продолжения из сокета.
pub fn parse_request(buf: &[u8]) -> Option<Request<'_>> {
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
    route(method, path, head)
}

/// Маршрут по методу и пути; отдельная функция, чтобы кадры разбора строки
/// запроса и поиска Content-Length не складывались в один выше порога
/// large_stack_frames.
fn route<'a>(method: &'a str, path: &'a str, head: &'a [u8]) -> Option<Request<'a>> {
    Some(match (method, path) {
        ("GET", "/") => Request::GetRoot,
        ("POST", "/ota") => {
            if cross_origin(head) {
                Request::CrossOrigin
            } else {
                match content_length(head) {
                    Some(content_length) => Request::PostOta { content_length },
                    None => Request::Malformed,
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

/// Похоже ли на запрос с чужой страницы (CSRF): браузерный `fetch` всегда
/// шлёт Origin, и у cross-origin запроса он не совпадает с Host; curl Origin
/// не шлёт вовсе. Отклонение запрещает странице из интернета вслепую
/// прошивать Гирлянду. Страница самой Гирлянды — same-origin (сервер только
/// http), поэтому сравнение с префиксом `http://` — без ложных срабатываний.
fn cross_origin(head: &[u8]) -> bool {
    let Some(origin) = header_value(head, b"Origin") else {
        return false;
    };
    let Some(host) = header_value(head, b"Host") else {
        return true;
    };
    match (core::str::from_utf8(origin), core::str::from_utf8(host)) {
        (Ok(origin), Ok(host)) => origin.strip_prefix("http://") != Some(host),
        // Origin/Host не текст — запрос не от браузера на страницу Гирлянды.
        _ => true,
    }
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

    #[test]
    fn get_root_recognized() {
        let bytes = b"GET / HTTP/1.1\r\nHost: garland\r\nAccept: text/html\r\n\r\n";
        assert_eq!(parse_request(bytes), Some(Request::GetRoot));
    }

    #[test]
    fn incomplete_head_is_none() {
        let bytes = b"GET / HTTP/1.1\r\nHost: garland\r\nAcc";
        assert_eq!(parse_request(bytes), None);
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
            parse_request(b"GET /ota HTTP/1.1\r\n\r\n"),
            Some(Request::Other {
                method: "GET",
                path: "/ota"
            })
        );
        assert_eq!(
            parse_request(b"POST / HTTP/1.1\r\nContent-Length: 5\r\n\r\nhello"),
            Some(Request::Other {
                method: "POST",
                path: "/"
            })
        );
    }

    #[test]
    fn post_ota_recognized_with_content_length() {
        assert_eq!(
            parse_request(b"POST /ota HTTP/1.1\r\nHost: g\r\nContent-Length: 1048576\r\n\r\n"),
            Some(Request::PostOta {
                content_length: 1_048_576
            })
        );
        // Браузеры шлют каноничное имя, но HTTP-заголовки регистронезависимы.
        assert_eq!(
            parse_request(b"POST /ota HTTP/1.1\r\ncontent-length: 12\r\n\r\n"),
            Some(Request::PostOta { content_length: 12 })
        );
        // Часть тела может прийти вместе с заголовком — разбору не мешает.
        assert_eq!(
            parse_request(b"POST /ota HTTP/1.1\r\nContent-Length: 4\r\n\r\n\xe9\x00\x02\x00"),
            Some(Request::PostOta { content_length: 4 })
        );
    }

    #[test]
    fn post_ota_without_content_length_is_malformed() {
        assert_eq!(
            parse_request(b"POST /ota HTTP/1.1\r\nHost: g\r\n\r\n"),
            Some(Request::Malformed)
        );
        assert_eq!(
            parse_request(b"POST /ota HTTP/1.1\r\nContent-Length: abc\r\n\r\n"),
            Some(Request::Malformed)
        );
    }

    #[test]
    fn post_ota_cross_origin_is_rejected() {
        // CSRF: страница из интернета пытается прошить Гирлянду вслепую.
        assert_eq!(
            parse_request(
                b"POST /ota HTTP/1.1\r\nHost: 192.168.4.1\r\nOrigin: http://evil.example\r\nContent-Length: 4\r\n\r\n"
            ),
            Some(Request::CrossOrigin)
        );
        // Origin с тем же адресом, но другим портом — тоже чужой.
        assert_eq!(
            parse_request(
                b"POST /ota HTTP/1.1\r\nHost: 192.168.4.1\r\nOrigin: http://192.168.4.1:8080\r\nContent-Length: 4\r\n\r\n"
            ),
            Some(Request::CrossOrigin)
        );
        // Origin без Host — отклоняем: обычный браузер Host всегда шлёт.
        assert_eq!(
            parse_request(b"POST /ota HTTP/1.1\r\nOrigin: http://g\r\nContent-Length: 4\r\n\r\n"),
            Some(Request::CrossOrigin)
        );
    }

    #[test]
    fn post_ota_same_origin_and_curl_are_accepted() {
        // Страница самой Гирлянды: Origin совпадает с Host (браузер не указывает
        // порт по умолчанию — сервер на 80-м).
        assert_eq!(
            parse_request(
                b"POST /ota HTTP/1.1\r\nHost: 192.168.4.1\r\nOrigin: http://192.168.4.1\r\nContent-Length: 4\r\n\r\n"
            ),
            Some(Request::PostOta { content_length: 4 })
        );
        // curl Origin не шлёт — не браузер, CSRF невозможен.
        assert_eq!(
            parse_request(b"POST /ota HTTP/1.1\r\nHost: g\r\nContent-Length: 4\r\n\r\n"),
            Some(Request::PostOta { content_length: 4 })
        );
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
        assert_eq!(parse_request(b"\x00\xff\r\n\r\n"), Some(Request::Malformed));
        assert_eq!(parse_request(b"GET\r\n\r\n"), Some(Request::Malformed));
    }

    #[test]
    fn version_page_is_valid_response() {
        let mut page = heapless::String::<RESPONSE_MAX>::new();
        write_page(&mut page, "1.2.3");
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
        // Форма заливки: файл уходит целиком, без multipart, методом POST /ota.
        assert!(body.contains("<form"));
        assert!(body.contains("type=\"file\""));
        assert!(body.contains("fetch('/ota'") || body.contains("fetch(\"/ota\""));
    }

    #[test]
    fn plain_answers_are_well_formed() {
        for (status, text) in [
            ("200 OK", TEXT_OTA_OK),
            ("400 Bad Request", TEXT_BAD_IMAGE),
            ("413 Payload Too Large", TEXT_PAYLOAD_TOO_LARGE),
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
