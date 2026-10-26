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
    /// Корректный HTTP-запрос с другим методом или путём.
    Other {
        /// Метод запроса (`GET`, `POST`, …).
        method: &'a str,
        /// Путь запроса (`/`, `/ota`, …).
        path: &'a str,
    },
    /// Начало потока не разбирается как HTTP-запрос.
    Malformed,
}

/// Вместимость буфера ответа страницы: заголовки + тело с версией (≈250 байт).
pub const RESPONSE_MAX: usize = 512;

/// Заголовок ответа до значения Content-Length.
const RESPONSE_HEAD: &str = "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\nContent-Length: ";

/// Конец заголовка ответа.
const RESPONSE_HEAD_END: &str = "\r\n\r\n";

/// Начало HTML-тела страницы (до версии Прошивки).
const PAGE_OPEN: &str = "<!doctype html><html><head><meta charset=\"utf-8\"><title>Гирлянда</title></head><body><h1>Гирлянда</h1><p>Версия Прошивки: ";

/// Конец HTML-тела страницы.
const PAGE_CLOSE: &str = "</p></body></html>";

/// Записать в буфер HTTP-ответ 200 со страницей версии Прошивки: заголовки
/// с верным Content-Length и тело с номером версии. Буфер очищается; сборка
/// push_str'ом, без core::fmt — кадр задачи HTTP-сервера остаётся маленьким.
pub fn write_version_page(page: &mut heapless::String<RESPONSE_MAX>, version: &str) {
    let body_len = PAGE_OPEN.len() + version.len() + PAGE_CLOSE.len();
    page.clear();
    page.push_str(RESPONSE_HEAD)
        .expect("страница помещается в буфер");
    push_usize(page, body_len);
    page.push_str(RESPONSE_HEAD_END)
        .expect("страница помещается в буфер");
    page.push_str(PAGE_OPEN)
        .expect("страница помещается в буфер");
    page.push_str(version).expect("страница помещается в буфер");
    page.push_str(PAGE_CLOSE)
        .expect("страница помещается в буфер");
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
    Some(match (method, path) {
        ("GET", "/") => Request::GetRoot,
        _ => Request::Other { method, path },
    })
}

/// Есть ли в буфере конец заголовка (`\r\n\r\n`): можно разбирать запрос.
pub fn head_complete(buf: &[u8]) -> bool {
    find_head_end(buf).is_some()
}

/// Первый индекс `\r\n\r\n` в буфере, если он там есть.
fn find_head_end(buf: &[u8]) -> Option<usize> {
    if buf.len() < 4 {
        return None;
    }
    (0..=buf.len() - 4).find(|&i| buf[i..i + 4] == *b"\r\n\r\n")
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
    fn garbage_is_malformed() {
        // Непохоже на request-line, но с валидным концом заголовка.
        assert_eq!(parse_request(b"\x00\xff\r\n\r\n"), Some(Request::Malformed));
        assert_eq!(parse_request(b"GET\r\n\r\n"), Some(Request::Malformed));
    }

    #[test]
    fn version_page_is_valid_response() {
        let mut page = heapless::String::<RESPONSE_MAX>::new();
        write_version_page(&mut page, "1.2.3");
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
    }

    #[test]
    fn rejects_are_well_formed() {
        for response in [RESPONSE_NOT_FOUND, RESPONSE_BAD_REQUEST] {
            let (head, body) = response.split_once("\r\n\r\n").expect("конец заголовка");
            let length = head
                .lines()
                .find_map(|l| l.strip_prefix("Content-Length: "))
                .expect("Content-Length");
            assert_eq!(length, body.len().to_string());
        }
        assert!(RESPONSE_NOT_FOUND.starts_with("HTTP/1.1 404 Not Found\r\n"));
        assert!(RESPONSE_BAD_REQUEST.starts_with("HTTP/1.1 400 Bad Request\r\n"));
    }
}
