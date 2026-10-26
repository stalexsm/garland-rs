//! Память Текущего эффекта: NVS-раздел таблицы разделов, вне Слотов —
//! обновление Прошивки её не затрагивает.
//!
//! Ключ-значение поверх раздела — sequential-storage (map API): циклическая
//! перезапись, износостойкость и живучесть при сбросе питания решены
//! проверенным крейтом; сам раздел берётся из таблицы через NorFlash-вид
//! esp-bootloader-esp-idf, мост в async-трейты — [`BlockingAsync`].
//!
//! Шов машины ([`EffectMemory`]) синхронный, а запись во флеш происходит
//! прямо при переключении: операции флеша esp-storage блокирующие сами по
//! себе, а внутри map нет ожиданий таймеров, поэтому [`block_on`] доводит
//! запись до конца за один опрос. Обычная запись — миллисекунды; при
//! заполнении страницы map стирает сектор флеша — редкий (раз в тысячи
//! переключений) блок на десятки миллисекунд внутри такта кадра. Стартовое
//! чтение — [`NvsMemory::new`].

use crate::control::EffectMemory;
use alloc::boxed::Box;
use embassy_embedded_hal::adapter::BlockingAsync;
use embassy_futures::block_on;
use esp_bootloader_esp_idf::partitions::{
    DataPartitionSubType, NorFlashRegion, PARTITION_TABLE_MAX_LEN, PartitionType,
    read_partition_table,
};
use esp_storage::FlashStorage;
use sequential_storage::cache::{Cache, Uncached};
use sequential_storage::map::{MapConfig, MapStorage};

/// Ключ записи в NVS: пока хранится один номер — нулевой.
const EFFECT_KEY: u8 = 0;

/// Скретч вызовов map: сериализация ключа и значения (два u8) с выравниванием
/// на слово флеша; запас — на служебные данные при саморемонте map.
const DATA_BUFFER_LEN: usize = 64;

/// Хранилище Текущего эффекта на NVS-разделе.
pub struct NvsMemory {
    /// Карта ключ-значение поверх NVS-раздела (кэш отключён: чтений мало).
    map: MapStorage<
        u8,
        BlockingAsync<NorFlashRegion<'static, 'static, 'static>>,
        Cache<Uncached, Uncached, Uncached, u8>,
    >,
    /// Скретч вызовов map: длина записи заведомо меньше буфера.
    buffer: [u8; DATA_BUFFER_LEN],
    /// Номер, прочитанный при старте; `None` — чистая Гирлянда.
    initial: Option<u8>,
}

impl NvsMemory {
    /// Хранилище на NVS-разделе флеша: раздел ищется в таблице, сохранённый
    /// номер читается сразу. Битая таблица или отсутствие раздела — ошибка
    /// развёртывания (паника на старте); ошибка чтения номера — как пустое
    /// хранилище: Гирлянда обязана светить и без памяти.
    pub async fn new(flash: FlashStorage<'static>) -> Self {
        // Флеш и регион уходят в кучу на всё время работы: цепочка
        // заимствований map → NorFlashRegion → FlashRegion → FlashStorage
        // становится 'static; объём — десятки байт, один раз на старте.
        let flash: &'static mut FlashStorage<'static> = Box::leak(Box::new(flash));
        // Буфер таблицы — в куче: 3 КБ на стеке задачи не нужны даже на время
        // старта (гейт large_stack_frames считает кадры от килобайта).
        let mut table_buffer = alloc::vec![0u8; PARTITION_TABLE_MAX_LEN];
        let table =
            read_partition_table(flash, &mut table_buffer).expect("таблица разделов не читается");
        let entry = table
            .find_partition(PartitionType::Data(DataPartitionSubType::Nvs))
            .expect("таблица разделов не читается")
            .expect("NVS-раздел отсутствует в таблице");
        let region: &'static mut _ = Box::leak(Box::new(entry.as_flash_region(flash)));
        let capacity = region.capacity();
        let nor_flash = region
            .as_nor_flash()
            .expect("NVS-раздел недоступен для записи");

        let mut this = Self {
            map: MapStorage::new(
                BlockingAsync::new(nor_flash),
                MapConfig::new(0..capacity as u32),
                Cache::new_uncached(),
            ),
            buffer: [0; DATA_BUFFER_LEN],
            initial: None,
        };
        this.initial = this
            .map
            .fetch_item::<u8>(&mut this.buffer, &EFFECT_KEY)
            .await
            .ok()
            .flatten();
        this
    }
}

impl EffectMemory for NvsMemory {
    fn save(&mut self, index: usize) {
        // Реестр вмещает до 256 эффектов; больший номер не сохраняется —
        // после перезапуска машина начнёт с первого, Гирлянда не ломается.
        let Some(value) = u8::try_from(index).ok() else {
            log::warn!("NVS: номер эффекта {index} не помещается в байт");
            return;
        };
        if let Err(error) = block_on(self.map.store_item::<u8>(
            &mut self.buffer,
            &EFFECT_KEY,
            &value,
        )) {
            log::warn!("NVS: номер эффекта не сохранён: {error:?}");
        }
    }

    fn load(&mut self) -> Option<usize> {
        self.initial.map(usize::from)
    }
}
