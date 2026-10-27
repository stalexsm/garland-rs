//! Запись OTA-образа в неактивный Слот.
//!
//! Транспорт-агностичный [`ImageWriter`]: принимает чанки тела HTTP-запроса
//! произвольной длины, буферизует их до флеш-сектора и пишет Слот выровненными
//! кусками — по одному стиранию на сектор. Стирание внутри не делается:
//! `esp_storage::FlashStorage::write` делает read-modify-write со стиранием
//! сам, а секторно-выровненные записи гарантируют ровно одно стирание на
//! сектор. Последний кусок добивается 0xFF до кратности 16 байт — требование
//! записи образа ESP-IDF (обязательное для шифрованной флеш).
//!
//! Логика буферизации чистая и проверяется хост-тестами на подставном
//! [`ImageTarget`]; настоящие носитель и подтверждение слота — device-часть
//! этого модуля и `net.rs`.

use sha2::{Digest, Sha256};

// Device-часть — только в прошивке; на хосте тестируется чистая половина.
#[cfg(target_arch = "riscv32")]
mod device {
    use crate::ota::{ImageTarget, ImageWriter, SECTOR_SIZE};
    use alloc::boxed::Box;
    use alloc::vec;
    use esp_bootloader_esp_idf::ota::OtaImageState;
    use esp_bootloader_esp_idf::ota_updater::OtaUpdater;
    use esp_bootloader_esp_idf::partitions::{
        AppPartitionSubType, Error, FlashRegion, PARTITION_TABLE_MAX_LEN,
    };
    use esp_storage::FlashStorage;
    use log::{info, warn};

    /// Магический байт первого байта образа ESP-IDF: отличает образ прошивки
    /// (espflash save-image) от случайного файла до активации.
    pub const IMAGE_MAGIC: u8 = 0xE9;

    /// Буфер чтения тела из сокета, байт.
    pub const CHUNK_SIZE: usize = 1024;

    /// Приёмник образа — неактивный Слот из таблицы разделов.
    pub struct RegionTarget<'a, 'd>(FlashRegion<'a, 'd>);

    impl<'a, 'd> RegionTarget<'a, 'd> {
        /// Обернуть регион раздела, полученный от [`Session::open_writer`].
        pub fn new(region: FlashRegion<'a, 'd>) -> Self {
            Self(region)
        }
    }

    impl ImageTarget for RegionTarget<'_, '_> {
        type Error = Error;

        fn capacity(&self) -> usize {
            self.0.capacity()
        }

        fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Error> {
            self.0.write(offset, bytes)
        }
    }

    /// Буферы OTA-сессии HTTP-сервера: уходят в кучу на всё время задачи
    /// ([`OtaBuffers::leak`]), как сокетные буферы в `net::http_task`.
    pub struct OtaBuffers {
        /// Буфер таблицы разделов.
        pub table: &'static mut [u8; PARTITION_TABLE_MAX_LEN],
        /// Секторный буфер образа.
        pub sector: &'static mut [u8; SECTOR_SIZE],
        /// Буфер чанка из сокета.
        pub chunk: &'static mut [u8; CHUNK_SIZE],
    }

    impl OtaBuffers {
        /// Буферы в куче; начальное содержимое не важно: каждый буфер
        /// перезаписывается до первого чтения. Конверсия Vec → Box<[u8; N]>
        /// держит временные на кадре крошечными — больших массивов на стеке
        /// нет.
        pub fn leak() -> Self {
            Self {
                table: Box::leak(
                    alloc::vec![0u8; PARTITION_TABLE_MAX_LEN]
                        .try_into()
                        .unwrap(),
                ),
                sector: Box::leak(alloc::vec![0u8; SECTOR_SIZE].try_into().unwrap()),
                chunk: Box::leak(alloc::vec![0u8; CHUNK_SIZE].try_into().unwrap()),
            }
        }
    }

    /// OTA-сессия поверх `OtaUpdater`: выбор неактивного Слота и активация
    /// после записи.
    pub struct Session<'a, 'd> {
        updater: OtaUpdater<'a, 'd>,
    }

    impl<'a, 'd> Session<'a, 'd> {
        /// Сессия по таблице разделов из флеша: требуются otadata и два
        /// OTA-Слота (канон `partitions.csv`); буфер — не менее
        /// `PARTITION_TABLE_MAX_LEN` байт.
        pub fn begin(flash: &'a mut FlashStorage<'d>, table: &'a mut [u8]) -> Result<Self, Error> {
            let table: &'a mut [u8; PARTITION_TABLE_MAX_LEN] = table
                .try_into()
                .expect("буфер таблицы разделов короче 0xC00");
            Ok(Self {
                updater: OtaUpdater::new(flash, table)?,
            })
        }

        /// Писатель в неактивный Слот; ёмкость Слота — `writer.capacity()`.
        pub fn open_writer<'b>(
            &'b mut self,
            sector: &'b mut [u8; SECTOR_SIZE],
        ) -> Result<ImageWriter<'b, RegionTarget<'b, 'd>>, Error> {
            let (region, _slot) = self.updater.next_partition()?;
            Ok(ImageWriter::new(RegionTarget::new(region), sector))
        }

        /// Активировать записанный Слот: otadata переключается на него,
        /// состояние — `New`: после перезагрузки Прошивка подтвердит себя
        /// сама ([`confirm`]), иначе загрузчик выполнит Откат.
        pub fn activate(&mut self) -> Result<(), Error> {
            self.updater.activate_next_partition()?;
            if let Err(error) = self.updater.set_current_ota_state(OtaImageState::New) {
                warn!("OTA: состояние New не записалось: {error:?}");
            }
            Ok(())
        }
    }

    /// Подтвердить текущий Слот после старта (`New`/`PendingVerify` →
    /// `Valid`): без подтверждения загрузчик с auto-rollback считает
    /// обновление неудачным и при следующем ребуте откатится. Заодно
    /// нормализует пустой otadata свежего устройства: крейт выбирает
    /// «следующий» Слот от otadata и без factory-раздела показал бы
    /// запущенный `ota_0` — писать образ поверх работающей Прошивки нельзя;
    /// запись otadata на запущенный первый Слот таблицы переводит выбор на
    /// `ota_1`.
    pub fn confirm(flash: &mut FlashStorage<'static>) {
        // Буфер таблицы — в куче: 3 КБ на кадре задачи не нужны (канон memory).
        let mut table = vec![0u8; PARTITION_TABLE_MAX_LEN];
        let mut session = match Session::begin(flash, &mut table) {
            Ok(session) => session,
            Err(error) => {
                warn!("OTA: таблица разделов не читается ({error:?}) — подтверждение пропущено");
                return;
            }
        };
        normalize_fresh_otadata(&mut session);
        mark_current_slot_valid(&mut session);
    }

    /// Пустой otadata свежего устройства — на запущенный первый Слот таблицы
    /// (`ota_0`): без этого крейт выбрал бы «следующим» запущенный Слот.
    fn normalize_fresh_otadata(session: &mut Session<'_, '_>) {
        // Доступ к выбранному Слоту и otadata — низкоуровневый `Ota`.
        let updater = &mut session.updater;
        if !matches!(
            updater.selected_partition(),
            Ok(AppPartitionSubType::Factory)
        ) {
            return;
        }
        match updater
            .ota_data()
            .and_then(|mut ota| ota.set_current_app_partition(AppPartitionSubType::Ota0))
        {
            Ok(()) => {
                info!("OTA: otadata пуст — выбран запущенный ota_0, обновление пойдёт в ota_1")
            }
            Err(error) => warn!("OTA: otadata не нормализован: {error:?}"),
        }
    }

    /// Подтвердить текущий Слот (`New`/`PendingVerify` → `Valid`).
    fn mark_current_slot_valid(session: &mut Session<'_, '_>) {
        let updater = &mut session.updater;
        match updater.current_ota_state() {
            Ok(OtaImageState::New) | Ok(OtaImageState::PendingVerify) => {
                match updater.set_current_ota_state(OtaImageState::Valid) {
                    Ok(()) => info!("OTA: слот подтверждён (Valid)"),
                    Err(error) => warn!("OTA: подтверждение не записалось: {error:?}"),
                }
            }
            Ok(state) => info!("OTA: слот уже подтверждён ({state:?})"),
            Err(error) => warn!("OTA: состояние слота не читается: {error:?}"),
        }
    }
}

#[cfg(target_arch = "riscv32")]
pub use device::{IMAGE_MAGIC, OtaBuffers, RegionTarget, Session, confirm};

/// Размер флеш-сектора ESP32-C6: единица стирания и буферизации образа.
pub const SECTOR_SIZE: usize = 4096;

/// Кратность последнего куска образа: паддинг 0xFF до 16 байт.
const IMAGE_PADDING: usize = 16;

/// Куда [`ImageWriter`] кладёт байты образа: Слот (устройство) или подставка
/// (хост-тесты).
pub trait ImageTarget {
    /// Ошибка носителя.
    type Error;

    /// Ёмкость Слота, байт.
    fn capacity(&self) -> usize;

    /// Записать `bytes` по смещению `offset` от начала Слота.
    fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error>;
}

/// Ошибка пополнения образа.
#[derive(Debug, PartialEq, Eq)]
pub enum ExtendError<E> {
    /// Образ не помещается в Слот; запись не начата.
    Overflow,
    /// Ошибка носителя.
    Target(E),
}

/// Пишущий конец OTA-сессии: сшивает чанки в секторные записи.
pub struct ImageWriter<'a, T: ImageTarget> {
    target: T,
    /// Буфер текущего сектора; живёт у владельца (кадр задачи HTTP-сервера).
    sector: &'a mut [u8; SECTOR_SIZE],
    /// Занято в [`Self::sector`].
    filled: usize,
    /// Записано в Слот, байт (смещение следующего сброса).
    written: u32,
    /// Потоковая сумма всех дописанных байт образа — сверяется с ожиданием
    /// клиента до активации Слота.
    hasher: Sha256,
}

impl<'a, T: ImageTarget> ImageWriter<'a, T> {
    /// Писатель в `target` со скретч-буфером сектора `sector`.
    pub fn new(target: T, sector: &'a mut [u8; SECTOR_SIZE]) -> Self {
        Self {
            target,
            sector,
            filled: 0,
            written: 0,
            hasher: Sha256::new(),
        }
    }

    /// Ёмкость Слота, байт: проверка размера образа до первой записи.
    pub fn capacity(&self) -> usize {
        self.target.capacity()
    }

    /// Дописать чанк образа. Чанк целиком не помещается в Слот — ошибка
    /// [`ExtendError::Overflow`] до любой записи.
    pub fn extend(&mut self, chunk: &[u8]) -> Result<(), ExtendError<T::Error>> {
        if self.written as usize + self.filled + chunk.len() > self.target.capacity() {
            return Err(ExtendError::Overflow);
        }
        let mut rest = chunk;
        while !rest.is_empty() {
            let take = core::cmp::min(SECTOR_SIZE - self.filled, rest.len());
            self.sector[self.filled..self.filled + take].copy_from_slice(&rest[..take]);
            self.hasher.update(&rest[..take]);
            self.filled += take;
            rest = &rest[take..];
            if self.filled == SECTOR_SIZE {
                self.flush().map_err(ExtendError::Target)?;
            }
        }
        Ok(())
    }

    /// Сумма SHA256 дописанных байт; сверяется с `X-OTA-SHA256` клиента до
    /// активации Слота. Паддинг [`Self::finish`] не считается.
    pub fn image_digest(&self) -> [u8; 32] {
        let mut digest = [0u8; 32];
        digest.copy_from_slice(self.hasher.clone().finalize().as_slice());
        digest
    }

    /// Завершить образ: добить 0xFF до кратности 16 байт (паддинг нужен
    /// только шифрованной записи, поэтому у края Слота он обрезается),
    /// сбросить остаток, вернуть размер образа без паддинга.
    pub fn finish(mut self) -> Result<u32, T::Error> {
        let image_len = self.written as usize + self.filled;
        let pad = (IMAGE_PADDING - image_len % IMAGE_PADDING) % IMAGE_PADDING;
        let pad = pad.min(self.target.capacity() - image_len);
        for _ in 0..pad {
            self.sector[self.filled] = 0xFF;
            self.filled += 1;
            if self.filled == SECTOR_SIZE {
                self.flush()?;
            }
        }
        self.flush()?;
        Ok(image_len as u32)
    }

    /// Сбросить буфер сектора в Слот.
    fn flush(&mut self) -> Result<(), T::Error> {
        if self.filled == 0 {
            return Ok(());
        }
        self.target
            .write(self.written, &self.sector[..self.filled])?;
        self.written += self.filled as u32;
        self.filled = 0;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// Состояние подставного Слота, доступное и после переезда в писатель.
    #[derive(Default)]
    struct State {
        flash: Vec<u8>,
        offsets: Vec<u32>,
    }

    /// Подставной Слот: записывает в вектор, помнит смещения вызовов.
    #[derive(Clone)]
    struct FakeTarget {
        capacity: usize,
        state: Rc<RefCell<State>>,
    }

    #[derive(Debug, PartialEq, Eq)]
    struct Full;

    impl FakeTarget {
        fn new(capacity: usize) -> Self {
            Self {
                capacity,
                state: Rc::new(RefCell::new(State {
                    flash: vec![0xFF; capacity],
                    offsets: Vec::new(),
                })),
            }
        }
    }

    impl ImageTarget for FakeTarget {
        type Error = Full;

        fn capacity(&self) -> usize {
            self.capacity
        }

        fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Full> {
            let mut state = self.state.borrow_mut();
            let start = offset as usize;
            if start + bytes.len() > state.flash.len() {
                return Err(Full);
            }
            state.flash[start..start + bytes.len()].copy_from_slice(bytes);
            state.offsets.push(offset);
            Ok(())
        }
    }

    /// Вектор SHA-256 строки «abc» из FIPS 180-2.
    const ABC_SHA256: [u8; 32] = [
        0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae, 0x22,
        0x23, 0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61, 0xf2, 0x00,
        0x15, 0xad,
    ];

    /// Сумма дописанных байт совпадает с вектором: по ней заливка сверяется
    /// до активации Слота.
    #[test]
    fn digest_matches_known_sha256_vector() {
        let target = FakeTarget::new(2 * SECTOR_SIZE);
        let mut sector = [0u8; SECTOR_SIZE];
        let mut writer = ImageWriter::new(target, &mut sector);
        writer.extend(b"abc").unwrap();
        assert_eq!(writer.image_digest(), ABC_SHA256);
    }

    /// Те же байты, пришедшие разными чанками через секторную границу,
    /// дают ту же сумму: чанки сшиваются в один поток.
    #[test]
    fn digest_is_chunking_invariant() {
        let one_shot = {
            let target = FakeTarget::new(3 * SECTOR_SIZE);
            let mut sector = [0u8; SECTOR_SIZE];
            let mut writer = ImageWriter::new(target, &mut sector);
            let image: Vec<u8> = (0..5000).map(|i| i as u8).collect();
            writer.extend(&image).unwrap();
            writer.extend(b"abc").unwrap();
            writer.image_digest()
        };
        let chunked = {
            let target = FakeTarget::new(3 * SECTOR_SIZE);
            let mut sector = [0u8; SECTOR_SIZE];
            let mut writer = ImageWriter::new(target, &mut sector);
            let image: Vec<u8> = (0..5000).map(|i| i as u8).collect();
            for chunk in image.chunks(17) {
                writer.extend(chunk).unwrap();
            }
            for byte in b"abc" {
                writer.extend(core::slice::from_ref(byte)).unwrap();
            }
            writer.image_digest()
        };
        assert_eq!(chunked, one_shot, "чанки не ломают поток байт");
    }

    #[test]
    fn small_image_lands_in_one_write_with_padding() {
        let target = FakeTarget::new(2 * SECTOR_SIZE);
        let handle = target.clone();
        let mut sector = [0u8; SECTOR_SIZE];
        let mut writer = ImageWriter::new(target, &mut sector);
        writer.extend(&[0xE9, 0, 2, 0]).unwrap();
        let len = writer.finish().unwrap();

        assert_eq!(len, 4);
        let state = handle.state.borrow();
        // Один сброс с начала Слота: образ + паддинг до 16 байт.
        assert_eq!(state.offsets, vec![0]);
        assert_eq!(&state.flash[..4], &[0xE9, 0, 2, 0]);
        assert!(state.flash[4..16].iter().all(|&b| b == 0xFF));
        // За пределами образа Слот остался стёртым.
        assert!(state.flash[16..].iter().all(|&b| b == 0xFF));
    }

    #[test]
    fn chunks_are_stitched_at_sector_boundaries() {
        let target = FakeTarget::new(3 * SECTOR_SIZE);
        let handle = target.clone();
        let mut sector = [0u8; SECTOR_SIZE];
        let mut writer = ImageWriter::new(target, &mut sector);
        let first: Vec<u8> = (0..5000).map(|i| i as u8).collect();
        writer.extend(&first).unwrap();
        writer.extend(&[7u8; 1000]).unwrap();
        let len = writer.finish().unwrap();

        assert_eq!(len, 6000);
        let state = handle.state.borrow();
        // Ровно два сброса: полный сектор и добитый до кратности 16 хвост.
        assert_eq!(state.offsets, vec![0, SECTOR_SIZE as u32]);
        assert_eq!(&state.flash[..4096], &first[..4096]);
        assert_eq!(&state.flash[4096..5000], &first[4096..]);
        assert_eq!(&state.flash[5000..6000], &[7u8; 1000]);
        assert!(state.flash[6000..6016].iter().all(|&b| b == 0xFF));
    }

    #[test]
    fn overflow_is_rejected_before_any_write() {
        let target = FakeTarget::new(100);
        let handle = target.clone();
        let mut sector = [0u8; SECTOR_SIZE];
        let mut writer = ImageWriter::new(target, &mut sector);
        assert_eq!(writer.capacity(), 100);
        assert_eq!(writer.extend(&[0u8; 101]), Err(ExtendError::Overflow));
        assert_eq!(writer.extend(&[1u8; 60]), Ok(()));
        // 60 + 41 больше ёмкости — отказ даже при буферизации.
        assert_eq!(writer.extend(&[2u8; 41]), Err(ExtendError::Overflow));
        writer.extend(&[3u8; 40]).unwrap();
        let len = writer.finish().unwrap();

        assert_eq!(len, 100);
        let state = handle.state.borrow();
        // Всё в одном буфере сектора — сброс один; паддинг, не влезающий в
        // Слот, обрезан: за образом остаётся стёртая хвостовка... она и есть
        // сам образ — до 100 байта включительно.
        assert_eq!(state.offsets, vec![0]);
        let expected = {
            let mut expected = vec![0u8; 100];
            expected[..60].fill(1);
            expected[60..].fill(3);
            expected
        };
        assert_eq!(state.flash, expected);
    }

    #[test]
    fn full_capacity_image_without_padding() {
        let target = FakeTarget::new(2 * SECTOR_SIZE);
        let handle = target.clone();
        let mut sector = [0u8; SECTOR_SIZE];
        let mut writer = ImageWriter::new(target, &mut sector);
        let image: Vec<u8> = (0..2 * SECTOR_SIZE).map(|i| i as u8).collect();
        writer.extend(&image).unwrap();
        let len = writer.finish().unwrap();

        assert_eq!(len, 2 * SECTOR_SIZE as u32);
        let state = handle.state.borrow();
        assert_eq!(state.offsets, vec![0, SECTOR_SIZE as u32]);
        assert_eq!(state.flash, image);
    }
    #[test]
    fn pad_never_crosses_sector_boundary() {
        // Сектор (4096) кратен 16, поэтому добивка до кратности 16 всегда
        // помещается в тот же сектор, где кончился образ: лишних сбросов нет.
        let target = FakeTarget::new(2 * SECTOR_SIZE);
        let handle = target.clone();
        let mut sector = [0u8; SECTOR_SIZE];
        let mut writer = ImageWriter::new(target, &mut sector);
        let image = vec![5u8; SECTOR_SIZE - 4]; // 4092: % 16 == 12 → паддинг 4
        writer.extend(&image).unwrap();
        let len = writer.finish().unwrap();

        assert_eq!(len, (SECTOR_SIZE - 4) as u32);
        let state = handle.state.borrow();
        assert_eq!(state.offsets, vec![0]);
        assert_eq!(state.flash[SECTOR_SIZE - 8..SECTOR_SIZE - 4], [5u8; 4]);
        assert_eq!(state.flash[SECTOR_SIZE - 4..SECTOR_SIZE], [0xFF; 4]);
    }
}
