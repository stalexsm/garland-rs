# Research: Эффекты AlexGyver (GyverMatrixOS и GyverMatrixBT) для портирования в garland-rs

*Дата исследования: 2026-02. Все ссылки на исходники — raw.githubusercontent.com, проверены прямым чтением файлов.*

**Две линейки прошивки AlexGyver для LED-матрицы (важно различать источники):**
- **GyverMatrixBT** (2018, v1.5/v1.6) — старая прошивка с управлением по Bluetooth HC-06. Именно её эффекты интересны для «матрицы в окне» — см. §4 и §5.
- **GyverMatrixOS** (2018–2019, v1.0…v1.15) — развитие BT-версии в «ОС для матрицы»: + шумовые эффекты, конфетти, часы, рассвет, анимации — см. §2, §3.

---

## 1. Репозитории и лицензия

### 1.1. Какие репозитории

**Репозиторий один на обе линейки: https://github.com/AlexGyver/GyverMatrixBT.** Страница проекта — https://alexgyver.ru/gyvermatrixbt/ (заголовок «Bluetooth адресная матрица на Arduino»); страница-гайд по прошивке — https://alexgyver.ru/gyvermatrixos-guide/. Прямой ссылки на GitHub в тексте гайда нет, но шапка скетча указывает репозиторий:

```cpp
// Скетч к проекту "Bluetooth управляемая RGB матрица"
// Страница проекта (схемы, описания): https://alexgyver.ru/GyverMatrixBT/
// Исходники на GitHub: https://github.com/AlexGyver/GyverMatrixBT/
// Автор: AlexGyver Technologies, 2018
```
Источник: https://raw.githubusercontent.com/AlexGyver/GyverMatrixBT/master/firmware/%D0%A1%D1%82%D0%B0%D1%80%D1%8B%D0%B5%20%D0%B2%D0%B5%D1%80%D1%81%D0%B8%D0%B8/GyverMatrixBT_v1.6/GyverMatrixBT_v1.6.ino

Структура папки `firmware/` (проверено через GitHub API, https://api.github.com/repos/AlexGyver/GyverMatrixBT/contents/firmware):
- `GyverMatrixOS_v1.12` — актуальная в этом репозитории версия GyverMatrixOS;
- `Старые версии/` — **GyverMatrixBT_v1.5, GyverMatrixBT_v1.6** (старая Bluetooth-линейка) и GyverMatrixOS_v1.0…v1.11.

**Более новые версии GyverMatrixOS** — в репозитории https://github.com/AlexGyver/GyverMatrixWiFi (v1.13–v1.15, WiFi-продолжение). Гайд сообщает: *«Версия с WiFi! Данный проект продолжил развивать vvip-68, добавилось управление по WiFi и куча всего нового!»*; шапка `GyverMatrixOS_v1.15.ino` ссылается на https://github.com/vvip-68/GyverMatrixWiFi (сам vvip-репозиторий помечен как устаревший в пользу https://github.com/vvip-68/GyverPanelWiFi). Страница https://alexgyver.ru/gyvermatrixbt/ тоже это подтверждает: *«Новая прошивка для матрицы называется GyverMatrixOS»*.

**Вывод:** за основу каталога GyverMatrixOS взят **v1.15** (https://raw.githubusercontent.com/AlexGyver/GyverMatrixWiFi/master/firmware/GyverMatrixOS_v1.15/GyverMatrixOS_v1.15.ino), для BT-линейки — **GyverMatrixBT_v1.6** (последняя BT-версия). Набор «классических» эффектов в OS идентичен BT (те же функции), кроме случаев, разобранных в §4.4.

### 1.2. Лицензия (прочитана дословно)

Файл LICENSE один на репозиторий и идентичен в обоих репозиториях AlexGyver:
- https://raw.githubusercontent.com/AlexGyver/GyverMatrixBT/master/LICENSE (покрывает и GyverMatrixBT_v1.6, и GyverMatrixOS_v1.12)
- https://raw.githubusercontent.com/AlexGyver/GyverMatrixWiFi/master/LICENSE

Дословная цитата (полный текст файла):

> **MIT License**
>
> Copyright (c) 2020 AlexGyver
>
> Permission is hereby granted, free of charge, to any person obtaining a copy
> of this software and associated documentation files (the "Software"), to deal
> in the Software without restriction, including without limitation the rights
> to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
> copies of the Software, and to permit persons to whom the Software is
> furnished to do so, subject to the following conditions:
>
> The above copyright notice and this permission notice shall be included in all
> copies or substantial portions of the Software.
>
> THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
> IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
> FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
> AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
> LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
> OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
> SOFTWARE.

### 1.3. Что это значит для garland-rs (частный проект без лицензии)

- MIT явно разрешает `use, copy, modify, merge` без ограничений — портировать алгоритмы **обеих** линеек (переписывать на Rust) можно свободно, в том числе в закрытый проект. Отсутствие LICENSE в garland-rs — не препятствие: MIT не copyleft, открывать наш код не требуется.
- Единственное условие — *«The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software»*: при копировании/существенной переработке сохранять уведомление об авторских правах. Для нераспространяемого частного кода внешнего триггера нет, но правильная практика — атрибуция в комментариях к портированным аддонам и в этом файле.
- Если garland-rs будет опубликован — приложить MIT-нотис «Copyright (c) 2020 AlexGyver» к аддонам с портированными эффектами. Нюанс по vvip-68 см. в §7 (п. 5).

---

## 2. GyverMatrixOS: полный список эффектов (v1.15)

### 2.1. Каталог (названия — как в прошивке)

Канонический список имён передаётся приложению строкой `EFFECT_LIST` в `GyverMatrixOS_v1.15.ino`:

```cpp
#define EFFECT_LIST F("Дыхание,Цвета,Снегопад,Шарик,Радуга,Радуга пикс,Огонь,The Matrix,Шарики,Часы,Звездопад,Конфетти,Радуга диагональная,Цветной шум,Облака,Лава,Плазма,Радужные переливы,Полосатые переливы,Зебра,Шумящий лес,Морской прибой,Лампа,Рассвет,Анимация 1,Анимация 2,Анимация 3,Анимация 4,Анимация 5")
```
Источник: https://raw.githubusercontent.com/AlexGyver/GyverMatrixWiFi/master/firmware/GyverMatrixOS_v1.15/GyverMatrixOS_v1.15.ino

Соответствие «имя → функция» задаётся в `custom.ino` (`customModes()`, `mapEffectToMode()`) и константах `EFFECT_*`. Описания — анализ исходников `effects.ino` / `noise_effects.ino` (интерпретация исследователя).

| # | Название | Функция | Что рисует |
|---|----------|---------|-----------|
| 0 | Дыхание | `brightnessRoutine()` | Плавное «дыхание» общей яркости (±2 за тик между 2 и globalBrightness) через `FastLED.setBrightness`. |
| 1 | Цвета | `colorsRoutine()` | Все зажжённые пиксели перекрашиваются в один цвет с медленно едущим hue (+4/тик). |
| 2 | Снегопад | `snowRoutine()` | Кадр сдвигается вниз; верхняя строка случайно засеивается бело-голубыми пикселями (`0xE0FFFF - 0x101010*random(0,4)`, шанс 1/SNOW_DENSE). |
| 3 | Шарик | `ballRoutine()` | Один квадрат BALL_SIZE×3 летает с отскоком от стен (субпиксельные координаты ×10), при отскоке меняет цвет (RANDOM_COLOR). |
| 4 | Радуга | `rainbowRoutine()` | Вертикальные полосы полного спектра `CHSV(hue + x*255/WIDTH)`, оттенок ползёт (+3/тик). |
| 5 | Радуга пикс | `rainbowColorsRoutine()` | То же, но только зажжённые пиксели (радуга «поверх рисунка»). |
| 6 | Огонь | `fireRoutine()` | Классический 2D-огонь: матрица значений 8×16 с масками оттенка/яркости, интерполяция кадров; угольки (SPARKLES). |
| 7 | The Matrix | `matrixRoutine()` | Зелёный «дождь»: в верхней строке случайно возникает зелёный (1/10), кадр ползёт вниз, зелёный канал гаснет на 0x20 за тик. |
| 8 | Шарики | `ballsRoutine()` | BALLS_AMOUNT=3 шара-пикселя с отскоком и хвостами (`fader(TRACK_STEP)`); 9 дискретных оттенков. |
| 9 | Часы | `clockRoutine()` | Цифровые часы 3×5; в v1.15 — наложение поверх эффектов (overlay). |
| 10 | Звездопад | `starfallRoutine()` | Кометы с левого края и нижней строки летят по диагонали, хвосты через `fadePixel(TAIL_STEP)`; цвет `CHSV(random(0,200), 150, 255)`. |
| 11 | Конфетти | `sparklesRoutine()` | Случайные яркие вспышки (DENSE=3/тик) на чёрном фоне, поле гаснет `fader(BRIGHT_STEP=70)`. |
| 12 | Радуга диагональная | `rainbowDiagonalRoutine()` | Диагональный градиент `CHSV(hue + (x+y)*255/max(W,H))`. |
| 13 | Цветной шум | `madnessNoise()` | Психоделика: hue и яркость пикселя — независимые `inoise8`-шумы. |
| 14 | Облака | `cloudNoise()` | Движок `fillNoiseLED()`, палитра `CloudColors_p`. |
| 15 | Лава | `lavaNoise()` | `fillNoiseLED()`, палитра `LavaColors_p`. |
| 16 | Плазма | `plasmaNoise()` | `fillNoiseLED()`, палитра `PartyColors_p`, colorLoop. |
| 17 | Радужные переливы | `rainbowNoise()` | `fillNoiseLED()`, палитра `RainbowColors_p`. |
| 18 | Полосатые переливы | `rainbowStripeNoise()` | `fillNoiseLED()`, палитра `RainbowStripeColors_p`. |
| 19 | Зебра | `zebraNoise()` | `fillNoiseLED()`, самодельная чёрно-белая палитра. |
| 20 | Шумящий лес | `forestNoise()` | `fillNoiseLED()`, палитра `ForestColors_p`. |
| 21 | Морской прибой | `oceanNoise()` | `fillNoiseLED()`, палитра `OceanColors_p`. |
| 22 | Лампа | `fillColorProcedure()` | Заливка всей матрицы одним цветом (с гамма-коррекцией). |
| 23 | Рассвет | `dawnProcedure()` | Будильник-рассвет: яркость 2→255 за ~20 мин, по спирали бежит «огонёк» (палитра LUT от красного к голубому). |
| 24–28 | Анимация 1–5 | `animation(1..5)` | Проигрывание гифок из `bitmap1.h`…`bitmap5.h`. |

Источники: https://raw.githubusercontent.com/AlexGyver/GyverMatrixWiFi/master/firmware/GyverMatrixOS_v1.15/effects.ino , https://raw.githubusercontent.com/AlexGyver/GyverMatrixWiFi/master/firmware/GyverMatrixOS_v1.15/noise_effects.ino , https://raw.githubusercontent.com/AlexGyver/GyverMatrixWiFi/master/firmware/GyverMatrixOS_v1.15/custom.ino

Системные режимы (не эффекты): бегущая строка (`DEMO_TEXT_0/1/2`, `fillString()`) и 6 игр — Змейка, Тетрис, Лабиринт, Runner, Flappy Bird, Арканоид (`GAME_LIST`, файлы `g_*.ino`).

### 2.2. Отличия v1.12 (репозиторий GyverMatrixBT) от v1.15

В `GyverMatrixOS_v1.12.ino` тот же базовый набор (константы `SNOW_ROUTINE … FIRE_ROUTINE`, 9 шумов) — https://raw.githubusercontent.com/AlexGyver/GyverMatrixBT/master/firmware/GyverMatrixOS_v1.12/GyverMatrixOS_v1.12.ino . В v1.13–v1.15 добавлены: наложение часов (`OVERLAY_CLOCK`), «Лампа», «Рассвет», 5 слотов анимаций, WiFi/NTP/будильник, индивидуальные скорости эффектов в EEPROM. Дефолты: v1.12 — `BRIGHTNESS 150`, `CURRENT_LIMIT 2000`; v1.15 — `BRIGHTNESS 32`, `CURRENT_LIMIT 1800` (шапки соответствующих .ino).

---

## 3. Топ-10 эффектов GyverMatrixOS: алгоритмы из исходников

Источник всех выдержек (если не указано иное): `firmware/GyverMatrixOS_v1.15/effects.ino` и `.../noise_effects.ino` репозитория https://github.com/AlexGyver/GyverMatrixWiFi. Выдержки сокращены без изменения кода, пропуски помечены `// ...`. Заметки о примитивах и порте — интерпретация исследователя. Эти же алгоритмы (с теми же масками и константами) присутствуют уже в GyverMatrixBT_v1.6 — расхождения отмечены в §4.4.

Хелперы AlexGyver (см. §6.4): `drawPixelXY`, `getPixColorXY`/`getPixColor`, `getPixelNumber`, `fillAll`, `fader`/`fadePixel`, `gammaCorrection`.

### 3.1. Огонь (fireRoutine) — главный эффект прошивки

Источник: https://raw.githubusercontent.com/AlexGyver/GyverMatrixWiFi/master/firmware/GyverMatrixOS_v1.15/effects.ino

Параметры: `SPARKLES 1` (угольки), `HUE_ADD 0` (сдвиг оттенка пламени 0–230); маски 8×16 в PROGMEM; шаг `pcnt += 30` (новая строка раз в 4 тика).

```cpp
// эффект "огонь"
#define SPARKLES 1        // вылетающие угольки вкл выкл
#define HUE_ADD 0         // добавка цвета в огонь (от 0 до 230) - меняет весь цвет пламени

unsigned char matrixValue[8][16];
unsigned char line[WIDTH];
int pcnt = 0;

//these values are substracetd from the generated values to give a shape to the animation
const unsigned char valueMask[8][16] PROGMEM = {
  {32 , 0  , 0  , 0  , 0  , 0  , 0  , 32 , 32 , 0  , 0  , 0  , 0  , 0  , 0  , 32 },
  // ... (8 строк, яркость растёт вниз, последняя строка до 255)
  {255, 192, 160, 128, 128, 160, 192, 255, 255, 192, 160, 128, 128, 160, 192, 255}
};

//these are the hues for the fire,
//should be between 0 (red) to about 25 (yellow)
const unsigned char hueMask[8][16] PROGMEM = {
  {1 , 11, 19, 25, 25, 22, 11, 1 , 1 , 11, 19, 25, 25, 22, 11, 1 },
  // ... (0=красный ... 25=жёлтый, центр горячее)
  {0 , 0 , 0 , 1 , 1 , 0 , 0 , 0 , 0 , 0 , 0 , 1 , 1 , 0 , 0 , 0 }
};

void fireRoutine() {
  if (loadingFlag) {
    modeCode = MC_FIRE;
    loadingFlag = false;
    FastLED.clear();
    generateLine();
    memset(matrixValue, 0, sizeof(matrixValue));
  }
  if (pcnt >= 100) {
    shiftUp();          // сдвиг матрицы значений вверх + новая строка
    generateLine();
    pcnt = 0;
  }
  drawFrame(pcnt);      // интерполяция между "ключевыми кадрами"
  pcnt += 30;
}

void generateLine() {
  for (uint8_t x = 0; x < WIDTH; x++) {
    line[x] = random(64, 255);      // яркость новой нижней строки 64..254
  }
}

void drawFrame(int pcnt) {
  int nextv;
  //each row interpolates with the one before it
  for (unsigned char y = HEIGHT - 1; y > 0; y--) {
    for (unsigned char x = 0; x < WIDTH; x++) {
      uint8_t newX = x;
      if (x > 15) newX = x%16;
      if (y < 8) {
        nextv =
          (((100.0 - pcnt) * matrixValue[y][newX]
            + pcnt * matrixValue[y - 1][newX]) / 100.0)
          - pgm_read_byte(&(valueMask[y][newX]));

        CRGB color = CHSV(
                       HUE_ADD + pgm_read_byte(&(hueMask[y][newX])), // H
                       255, // S
                       (uint8_t)max(0, nextv) // V
                     );

        leds[getPixelNumber(x, y)] = color;
      } else if (y == 8 && SPARKLES) {
        if (random(0, 20) == 0 && getPixColorXY(x, y - 1) != 0) drawPixelXY(x, y, getPixColorXY(x, y - 1));
        else drawPixelXY(x, y, 0);
      } else if (SPARKLES) {
        if (getPixColorXY(x, y - 1) > 0)
          drawPixelXY(x, y, getPixColorXY(x, y - 1));
        else drawPixelXY(x, y, 0);
      }
    }
  }

  //first row interpolates with the "next" line
  for (unsigned char x = 0; x < WIDTH; x++) {
    uint8_t newX = x;
    if (x > 15) newX = x%16;
    CRGB color = CHSV(
                   HUE_ADD + pgm_read_byte(&(hueMask[0][newX])), // H
                   255,           // S
                   (uint8_t)(((100.0 - pcnt) * matrixValue[0][newX] + pcnt * line[newX]) / 100.0) // V
                 );
    //leds[getPixelNumber(newX, 0)] = color; // На форуме пишут что это ошибка - вместо newX должно быть x, иначе
    leds[getPixelNumber(x, 0)] = color;      // на матрицах шире 16 столбцов нижний правый угол неработает
  }
}
```

Примитивы FastLED: `CHSV` (H 0..25 — красный→жёлтый), `CRGB`, `FastLED.clear`, `pgm_read_byte`/`PROGMEM`, `random()`. Порт: маски жёстко 8×16 — на 8×8 берётся один «факел» (столбцы 0–7); float-интерполяция заменяется на целочисленную `((100-pcnt)*a + pcnt*b)/100`; `random(64,255)` — верхняя граница исключена (интерпретация исследователя).

### 3.2. The Matrix (matrixRoutine)

Источник: https://raw.githubusercontent.com/AlexGyver/GyverMatrixWiFi/master/firmware/GyverMatrixOS_v1.15/effects.ino

```cpp
void matrixRoutine() {
  if (loadingFlag) {
    loadingFlag = false;
    modeCode = MC_MATRIX;
    FastLED.clear();
  }
  for (byte x = 0; x < WIDTH; x++) {
    // заполняем случайно верхнюю строку
    uint32_t thisColor = getPixColorXY(x, HEIGHT - 1);
    if (thisColor == 0)
      drawPixelXY(x, HEIGHT - 1, 0x00FF00 * (random(0, 10) == 0));
    else if (thisColor < 0x002000)
      drawPixelXY(x, HEIGHT - 1, 0);
    else
      drawPixelXY(x, HEIGHT - 1, thisColor - 0x002000);
  }

  // сдвигаем всё вниз
  for (byte x = 0; x < WIDTH; x++) {
    for (byte y = 0; y < HEIGHT - 1; y++) {
      drawPixelXY(x, y, getPixColorXY(x, y + 1));
    }
  }
}
```

Цвет `0x00FF00`, вероятность 1/10 на столбец за тик, затухание −0x002000 (зелёный канал −32/тик, след ~8 тиков). Работает благодаря хранению цвета как `uint32_t` 0xRRGGBB в `getPixColor`.

### 3.3. Шарики (ballsRoutine)

Источник: https://raw.githubusercontent.com/AlexGyver/GyverMatrixWiFi/master/firmware/GyverMatrixOS_v1.15/effects.ino

Параметры: `BALLS_AMOUNT 3`, `BALL_TRACK 1`, `TRACK_STEP 70`, `CLEAR_PATH 1`.

```cpp
// эффект "шарики"
#define BALLS_AMOUNT 3    // количество "шариков"
#define BALL_TRACK 1      // (0 / 1) - вкл/выкл следы шариков
#define TRACK_STEP 70     // длина хвоста шарика (чем больше цифра, тем хвост короче)

int coord[BALLS_AMOUNT][2];
int8_t vector[BALLS_AMOUNT][2];
CRGB ballColors[BALLS_AMOUNT];

void ballsRoutine() {
  if (loadingFlag) {
    modeCode = MC_BALLS;
    loadingFlag = false;
    for (byte j = 0; j < BALLS_AMOUNT; j++) {
      int sign;
      // забиваем случайными данными
      coord[j][0] = WIDTH / 2 * 10;
      random(0, 2) ? sign = 1 : sign = -1;
      vector[j][0] = random(4, 15) * sign;
      coord[j][1] = HEIGHT / 2 * 10;
      random(0, 2) ? sign = 1 : sign = -1;
      vector[j][1] = random(4, 15) * sign;
      ballColors[j] = CHSV(random(0, 9) * 28, 255, 255);
    }
  }

  if (!BALL_TRACK)    // если режим БЕЗ следов шариков
    FastLED.clear();  // очистить
  else {              // режим со следами
    fader(TRACK_STEP);
  }

  // движение шариков
  for (byte j = 0; j < BALLS_AMOUNT; j++) {
    for (byte i = 0; i < 2; i++) {
      coord[j][i] += vector[j][i];
      if (coord[j][i] < 0) {
        coord[j][i] = 0;
        vector[j][i] = -vector[j][i];
      }
    }
    if (coord[j][0] > (WIDTH - 1) * 10) {
      coord[j][0] = (WIDTH - 1) * 10;
      vector[j][0] = -vector[j][0];
    }
    if (coord[j][1] > (HEIGHT - 1) * 10) {
      coord[j][1] = (HEIGHT - 1) * 10;
      vector[j][1] = -vector[j][1];
    }
    leds[getPixelNumber(coord[j][0] / 10, coord[j][1] / 10)] =  ballColors[j];
  }
}

// функция плавного угасания цвета для всех пикселей
void fader(byte step) {
  for (byte i = 0; i < WIDTH; i++) {
    for (byte j = 0; j < HEIGHT; j++) {
      fadePixel(i, j, step);
    }
  }
}

void fadePixel(byte i, byte j, byte step) {     // новый фейдер
  int pixelNum = getPixelNumber(i, j);
  if (getPixColor(pixelNum) == 0) return;
  if (leds[pixelNum].r >= 30 ||
      leds[pixelNum].g >= 30 ||
      leds[pixelNum].b >= 30) {
    leds[pixelNum].fadeToBlackBy(step);
  } else {
    leds[pixelNum] = 0;
  }
}
```

Примитивы: `fadeToBlackBy(step)` — единственное место в прошивке; `CHSV(rnd*28)` — 9 дискретных цветов; скорость 0.4–1.4 px/тик. Полностью целочисленная — портится почти 1:1.

### 3.4. Звездопад (starfallRoutine)

Источник: https://raw.githubusercontent.com/AlexGyver/GyverMatrixWiFi/master/firmware/GyverMatrixOS_v1.15/effects.ino

Параметры: `STAR_DENSE 60`, `SATURATION 150`, `TAIL_STEP 100`.

```cpp
void starfallRoutine() {
  if (loadingFlag) {
    modeCode = MC_STARFALL;
    loadingFlag = false;
    FastLED.clear();  // очистить
  }

  // заполняем головами комет левую и верхнюю линию
  for (byte i = HEIGHT / 2; i < HEIGHT; i++) {
    if (getPixColorXY(0, i) == 0
        && (random(0, STAR_DENSE) == 0)
        && getPixColorXY(0, i + 1) == 0
        && getPixColorXY(0, i - 1) == 0)
      leds[getPixelNumber(0, i)] = CHSV(random(0, 200), SATURATION, 255);
  }

  for (byte i = 0; i < WIDTH / 2; i++) {
    if (getPixColorXY(i, HEIGHT - 1) == 0
        && (random(0, STAR_DENSE) == 0)
        && getPixColorXY(i + 1, HEIGHT - 1) == 0
        && getPixColorXY(i - 1, HEIGHT - 1) == 0)
      leds[getPixelNumber(i, HEIGHT - 1)] = CHSV(random(0, 200), SATURATION, 255);
  }

  // сдвигаем по диагонали
  for (byte y = 0; y < HEIGHT - 1; y++) {
    for (byte x = WIDTH - 1; x > 0; x--) {
      drawPixelXY(x, y, getPixColorXY(x - 1, y + 1));
    }
  }

  // уменьшаем яркость левой и верхней линии, формируем "хвосты"
  for (byte i = HEIGHT / 2; i < HEIGHT; i++) {
    fadePixel(0, i, TAIL_STEP);
  }
  for (byte i = 0; i < WIDTH / 2; i++) {
    fadePixel(i, HEIGHT - 1, TAIL_STEP);
  }
}
```

Кометы зарождаются на левом крае (нижняя половина) и нижней строке (левая половина), летят по диагонали вправо-вверх; комментарий в коде («левую и верхнюю») не соответствует координатам — расхождение отмечено в §7 (п. 3).

### 3.5. Конфетти (sparklesRoutine)

Источник: https://raw.githubusercontent.com/AlexGyver/GyverMatrixWiFi/master/firmware/GyverMatrixOS_v1.15/effects.ino

Параметры: `DENSE 3`, `BRIGHT_STEP 70`.

```cpp
#define DENSE 3           // плотность конфетти
#define BRIGHT_STEP 70    // шаг уменьшения яркости

// рандомные гаснущие вспышки
void sparklesRoutine() {
  if (loadingFlag) {
    modeCode = MC_SPARKLES;
    loadingFlag = false;
    FastLED.clear();  // очистить
  }
  for (byte i = 0; i < DENSE; i++) {
    byte x = random(0, WIDTH);
    byte y = random(0, HEIGHT);
    if (getPixColorXY(x, y) == 0)
      leds[getPixelNumber(x, y)] = CHSV(random(0, 255), 255, 255);
  }
  fader(BRIGHT_STEP);
}
```

Простейший эффект, идеален как первый порт.

### 3.6. Снегопад (snowRoutine)

Источник: https://raw.githubusercontent.com/AlexGyver/GyverMatrixWiFi/master/firmware/GyverMatrixOS_v1.15/effects.ino

Параметр: `SNOW_DENSE 10`.

```cpp
#define SNOW_DENSE 10     // плотность снегопада

void snowRoutine() {
  if (loadingFlag) {
    modeCode = MC_SNOW;
    loadingFlag = false;
    FastLED.clear();  // очистить
  }

  // сдвигаем всё вниз
  for (byte x = 0; x < WIDTH; x++) {
    for (byte y = 0; y < HEIGHT - 1; y++) {
      drawPixelXY(x, y, getPixColorXY(x, y + 1));
    }
  }

  for (byte x = 0; x < WIDTH; x++) {
    // заполняем случайно верхнюю строку
    // а также не даём двум блокам по вертикали вместе быть
    if (getPixColorXY(x, HEIGHT - 2) == 0 && (random(0, SNOW_DENSE) == 0))
      drawPixelXY(x, HEIGHT - 1, 0xE0FFFF - 0x101010 * random(0, 4));
    else
      drawPixelXY(x, HEIGHT - 1, 0x000000);
  }
}
```

Палитра снежинок: `0xE0FFFF - 0x101010*k`, k=0..3 (R=224, G=B=255−16k) — бело-голубые; «слипание» предотвращается проверкой `(x, HEIGHT-2) == 0`.

### 3.7. Радуга диагональная (rainbowDiagonalRoutine)

Источник: https://raw.githubusercontent.com/AlexGyver/GyverMatrixWiFi/master/firmware/GyverMatrixOS_v1.15/effects.ino

```cpp
void rainbowDiagonalRoutine() {
  if (loadingFlag) {
    modeCode = MC_RAINBOW_DIAG;
    loadingFlag = false;
    FastLED.clear();  // очистить
  }
  hue += 3;
  for (byte x = 0; x < WIDTH; x++) {
    for (byte y = 0; y < HEIGHT; y++) {
      CRGB thisColor = CHSV((byte)(hue + (float)(WIDTH / HEIGHT * x + y) * (float)(255 / max(WIDTH, HEIGHT))), 255, 255);
      drawPixelXY(x, y, thisColor);
    }
  }
}
```

`hue += 3` за тик; шаг градиента `255/max(W,H)`. **Внимание:** float-выражение с целочисленными делениями внутри (`WIDTH/HEIGHT`, `255/max(W,H)`); при WIDTH==HEIGHT член `WIDTH/HEIGHT*x` = x, т.е. фактически `hue + (x+y)*(255/max(W,H))` (интерпретация исследователя). Для порта — заранее вычисленный целый шаг.

### 3.8. Цветной шум (madnessNoise)

Источник: https://raw.githubusercontent.com/AlexGyver/GyverMatrixWiFi/master/firmware/GyverMatrixOS_v1.15/noise_effects.ino

```cpp
// "масштаб" эффектов. Чем меньше, тем крупнее!
#define MADNESS_SCALE 100

static uint16_t x;
static uint16_t y;
static uint16_t z;

uint16_t speed = 20;
uint16_t scale = 30;

#if (WIDTH > HEIGHT)
uint8_t noise[WIDTH][WIDTH];
#else
uint8_t noise[HEIGHT][HEIGHT];
#endif

void madnessNoise() {
  if (loadingFlag) {
    loadingFlag = false;
    scale = MADNESS_SCALE;
    modeCode = MC_NOISE_MADNESS;
  }
  fillnoise8();
  for (int i = 0; i < WIDTH; i++) {
    for (int j = 0; j < HEIGHT; j++) {
      CRGB thisColor = CHSV(noise[j][i], 255, noise[i][j]);
      drawPixelXY(i, j, thisColor);
    }
  }
  ihue += 1;
}

void fillnoise8() {
  for (int i = 0; i < MAX_DIMENSION; i++) {
    int ioffset = scale * i;
    for (int j = 0; j < MAX_DIMENSION; j++) {
      int joffset = scale * j;
      noise[i][j] = inoise8(x + ioffset, y + joffset, z);
    }
  }
  z += speed;
}
```

Примитивы: **`inoise8`** (целочисленный 8-битный шум Перлина), `CHSV`. Дрейф только `z += speed` (speed=20). Hue из `noise[j][i]` (транспонированный буфер — радужная рябь), яркость из `noise[i][j]`. Буфер — max(W,H)² байт (64 байта на 8×8).

### 3.9. Радужные переливы и семейство палитровых шумов (fillNoiseLED)

Источник: https://raw.githubusercontent.com/AlexGyver/GyverMatrixWiFi/master/firmware/GyverMatrixOS_v1.15/noise_effects.ino

Один движок для 8 эффектов (Облака, Лава, Плазма, Радужные переливы, Полосатые переливы, Зебра, Шумящий лес, Морской прибой) — различаются палитра, scale, colorLoop:

```cpp
#define CLOUD_SCALE 30
#define LAVA_SCALE 50
#define PLASMA_SCALE 30
#define RAINBOW_SCALE 30
#define RAINBOW_S_SCALE 20
#define ZEBRA_SCALE 30
#define FOREST_SCALE 120
#define OCEAN_SCALE 90

CRGBPalette16 currentPalette( PartyColors_p );
uint8_t colorLoop = 1;
uint8_t ihue = 0;

void rainbowNoise() {
  if (loadingFlag) {
    loadingFlag = false;
    currentPalette = RainbowColors_p;
    scale = RAINBOW_SCALE; colorLoop = 1;
    modeCode = MC_NOISE_RAINBOW;
  }
  fillNoiseLED();
}
// ... cloudNoise (CloudColors_p, colorLoop 0), lavaNoise (LavaColors_p, 0),
//     plasmaNoise (PartyColors_p, 1), zebraNoise (белый в слотах 0/4/8/12, 1),
//     forestNoise (ForestColors_p, 0), oceanNoise (OceanColors_p, 0) ...

void fillNoiseLED() {
  uint8_t dataSmoothing = 0;
  if ( speed < 50) {
    dataSmoothing = 200 - (speed * 4);
  }
  for (int i = 0; i < MAX_DIMENSION; i++) {
    int ioffset = scale * i;
    for (int j = 0; j < MAX_DIMENSION; j++) {
      int joffset = scale * j;

      uint8_t data = inoise8(x + ioffset, y + joffset, z);

      data = qsub8(data, 16);
      data = qadd8(data, scale8(data, 39));

      if ( dataSmoothing ) {
        uint8_t olddata = noise[i][j];
        uint8_t newdata = scale8( olddata, dataSmoothing) + scale8( data, 256 - dataSmoothing);
        data = newdata;
      }

      noise[i][j] = data;
    }
  }
  z += speed;

  // apply slow drift to X and Y, just for visual variation.
  x += speed / 8;
  y -= speed / 16;

  for (int i = 0; i < WIDTH; i++) {
    for (int j = 0; j < HEIGHT; j++) {
      uint8_t index = noise[j][i];
      uint8_t bri =   noise[i][j];
      if ( colorLoop) {
        index += ihue;
      }
      if ( bri > 127 ) {
        bri = 255;
      } else {
        bri = dim8_raw( bri * 2);
      }
      CRGB color = ColorFromPalette( currentPalette, index, bri);
      drawPixelXY(i, j, color);
    }
  }
  ihue += 1;
}
```

Примитивы FastLED: `inoise8`, `qsub8`, `qadd8`, `scale8`, `dim8_raw`, `ColorFromPalette`, `CRGBPalette16` + стандартные палитры (`RainbowColors_p`, `RainbowStripeColors_p`, `PartyColors_p`, `CloudColors_p`, `LavaColors_p`, `ForestColors_p`, `OceanColors_p`), `fill_solid` (зебра). При speed=20: `dataSmoothing=120`, дрейф `z += 20; x += 2; y -= 1`, `ihue += 1` за тик. Все операции целочисленные saturating-примитивы — для no_std реализуются вручную (qadd8/qsub8 — saturating add/sub u8, scale8 — `(a*b)>>8`).

### 3.10. Шарик / «блуждающий кубик» (ballRoutine)

Источник: https://raw.githubusercontent.com/AlexGyver/GyverMatrixWiFi/master/firmware/GyverMatrixOS_v1.15/effects.ino

Параметры: `BALL_SIZE 3`, `RANDOM_COLOR 1`.

```cpp
// эффект "квадратик"
#define BALL_SIZE 3       // размер квадрата
#define RANDOM_COLOR 1    // случайный цвет при отскоке

int coordB[2];
int8_t vectorB[2];
CRGB ballColor;

void ballRoutine() {
  if (loadingFlag) {
    for (byte i = 0; i < 2; i++) {
      coordB[i] = WIDTH / 2 * 10;
      vectorB[i] = random(8, 20);
      ballColor = CHSV(random(0, 9) * 28, 255, 255);
    }
    modeCode = MC_BALL;
    loadingFlag = false;
  }
  for (byte i = 0; i < 2; i++) {
    coordB[i] += vectorB[i];
    if (coordB[i] < 0) {
      coordB[i] = 0;
      vectorB[i] = -vectorB[i];
      if (RANDOM_COLOR) ballColor = CHSV(random(0, 9) * 28, 255, 255);
    }
  }
  if (coordB[0] > (WIDTH - BALL_SIZE) * 10) {
    coordB[0] = (WIDTH - BALL_SIZE) * 10;
    vectorB[0] = -vectorB[0];
    if (RANDOM_COLOR) ballColor = CHSV(random(0, 9) * 28, 255, 255);
  }
  if (coordB[1] > (HEIGHT - BALL_SIZE) * 10) {
    coordB[1] = (HEIGHT - BALL_SIZE) * 10;
    vectorB[1] = -vectorB[1];
    if (RANDOM_COLOR) ballColor = CHSV(random(0, 9) * 28, 255, 255);
  }
  FastLED.clear();
  for (byte i = 0; i < BALL_SIZE; i++)
    for (byte j = 0; j < BALL_SIZE; j++)
      leds[getPixelNumber(coordB[0] / 10 + i, coordB[1] / 10 + j)] = ballColor;
}
```

Скорость 0.8–1.9 px/тик, старт из центра, 9 дискретных цветов. Полностью целочисленный.

**Примитивы FastLED, реально используемые в GyverMatrixOS:** `CHSV`/`CRGB`, `inoise8`, `fadeToBlackBy`, `ColorFromPalette` + `CRGBPalette16`/`*_Colors_p`, `qadd8`/`qsub8`/`scale8`/`dim8_raw`, `fill_solid`, `FastLED.clear/setBrightness/show/addLeds/setMaxPowerInVoltsAndMilliamps`. Примитивы `beat88`, `sin8`, `blend`, `random8`, `hsv2rgb_rainbow` (в явном виде) здесь **не используются** — они характерны для GyverLamp (https://github.com/AlexGyver/GyverLamp, другая прошивка). «Шторм», «светлячки», «мета-шары» в GyverMatrixOS/GyverMatrixBT отсутствуют — если нужны, это отдельное исследование GyverLamp.

---

## 4. GyverMatrixBT v1.6: старая Bluetooth-прошивка

### 4.1. Где лежит

- Страница проекта: https://alexgyver.ru/gyvermatrixbt/ ; репозиторий: https://github.com/AlexGyver/GyverMatrixBT , папка `firmware/Старые версии/GyverMatrixBT_v1.6/` (предыдущая — `GyverMatrixBT_v1.5`). Лицензия — та же MIT © 2020 AlexGyver (§1.2), шапка скетча: «Скетч к проекту "Bluetooth управляемая RGB матрица"… Автор: AlexGyver Technologies, 2018».
- Файлы v1.6: `GyverMatrixBT_v1.6.ino` (настройки/глобалы), `effects.ino` (все эффекты), `custom.ino` (демо-цикл), `bluetooth.ino`, `buttons.ino`, `runningText.ino`, `snake.ino`, `tetris.ino`, `maze.ino`, `utility.ino`, `fonts.h` (листинг: https://api.github.com/repos/AlexGyver/GyverMatrixBT/contents/firmware/%D0%A1%D1%82%D0%B0%D1%80%D1%8B%D0%B5%20%D0%B2%D0%B5%D1%80%D1%81%D0%B8%D0%B8/GyverMatrixBT_v1.6).
- Raw-ссылки (путь с кириллицей URL-кодируется): 
  - главный: https://raw.githubusercontent.com/AlexGyver/GyverMatrixBT/master/firmware/%D0%A1%D1%82%D0%B0%D1%80%D1%8B%D0%B5%20%D0%B2%D0%B5%D1%80%D1%81%D0%B8%D0%B8/GyverMatrixBT_v1.6/GyverMatrixBT_v1.6.ino
  - эффекты: https://raw.githubusercontent.com/AlexGyver/GyverMatrixBT/master/firmware/%D0%A1%D1%82%D0%B0%D1%80%D1%8B%D0%B5%20%D0%B2%D0%B5%D1%80%D1%81%D0%B8%D0%B8/GyverMatrixBT_v1.6/effects.ino
  - демо-цикл: https://raw.githubusercontent.com/AlexGyver/GyverMatrixBT/master/firmware/%D0%A1%D1%82%D0%B0%D1%80%D1%8B%D0%B5%20%D0%B2%D0%B5%D1%80%D1%81%D0%B8%D0%B8/GyverMatrixBT_v1.6/custom.ino

Настройки v1.6: `WIDTH 16`, `HEIGHT 16`, `BRIGHTNESS 60`, `D_EFFECT_SPEED 100` мс, таймеры `GTimer_ms` (библиотека GyverTimer.h), `randomSeed(analogRead(0) + analogRead(1))` — источник главного файла выше.

### 4.2. Полный список эффектов GyverMatrixBT v1.6

Официальный демо-цикл — `custom.ino` (комментарии из кода):

```cpp
/*
   Эффекты:
    snowRoutine();      // снег
    matrixRoutine();    // "матрица"
    starfallRoutine();  // звездопад (кометы)
    ballRoutine();      // квадратик
    ballsRoutine();     // шарики
    wavesRoutine();     // синусоиды
    rainbowRoutine();   // радуга во всю матрицу
    fireRoutine();      // огонь
*/
#define MODES_AMOUNT 11     // количество кастомных режимов
void customModes() {
  switch (thisMode) {
    case 0: snowRoutine();      break;
    case 1: matrixRoutine();    break;
    case 2: starfallRoutine();  break;
    case 3: ballRoutine();      break;
    case 4: ballsRoutine();     break;
    case 5: wavesRoutine();     break;
    case 6: rainbowRoutine();   break;
    case 7: fireRoutine();      break;
    case 8: snakeRoutine();     break;   // игра
    case 9: tetrisRoutine();    break;   // игра
    case 10: mazeRoutine();     break;   // игра
    case 11: fillString("Ваш текст");    break;  // бегущая строка
  }
  FastLED.show();
}
```

Кроме демо-цикла, в `effects.ino` есть ещё три routine (вызываются командами BT при ручном управлении): `brightnessRoutine()` («"дыхание" яркостью»), `colorsRoutine()` («смена цвета активных светодиодов (рисунка)»), `rainbowColorsRoutine()` («радуга активных светодиодов (рисунка)»). Названия в README/приложении эпохи BT (https://github.com/AlexGyver/GyverMatrixBT — раздел «Эффекты»): «"Дыхание" яркости, Смена цвета, Снегопад, Блуждающий кубик, Радуга, Огонь, The Matrix, Летающие частицы, Линии». Соответствие имён коду: «Летающие частицы» → `ballsRoutine`, «Линии» → `wavesRoutine` (интерпретация исследователя по описанию функций; в коде имена — «шарики» и «синусоиды»).

Каталог эффектов BT v1.6 (описания — по коду `effects.ino` v1.6):

| Демо-№ | Название (код / приложение) | Функция | Что рисует |
|---|---|---|---|
| 0 | Снегопад 2.0 / «Снегопад» | `snowRoutine()` | Тот же алгоритм, что в OS (§3.6), но плотность захардкожена: `random(0, 10)`. |
| 1 | Матрица / «The Matrix» | `matrixRoutine()` | Идентичен OS-версии (§3.2). |
| 2 | Звездопад (кометы) / — | `starfallRoutine()` | Алгоритм тот же (§3.4); в v1.6 шанс спавна захардкожен `random(0, 60)`, шаг хвоста несимметричен: левый край гаснет `fadePixel(0, i, TRACK_STEP)` (40), низ — `fadePixel(i, HEIGHT-1, TAIL_STEP)` (30); в OS оба TAIL_STEP. |
| 3 | Блудный кубик / «Блуждающий кубик» | `ballRoutine()` | Как в OS (§3.10), но медленнее: `vectorB[i] = random(3, 8)` (OS: `random(8, 20)`). |
| 4 | Шарики / «Летающие частицы» | `ballsRoutine()` | Как в OS (§3.3), плюс **уникальный режим `DRAW_WALLS 1`** — отскок от нарисованных пользователем препятствий (см. §4.3); `TRACK_STEP 40` (OS: 70). |
| 5 | Синусоиды / «Линии» | `wavesRoutine()` | **Уникальный эффект BT** (в OS удалён): 2 синусоиды, бегущие вправо; см. §4.3. |
| 6 | Радуга (заливка) / «Радуга» | `rainbowRoutine()` | Как в OS, но `hue++` за тик (OS: `hue += 3`). |
| 7 | Огонь / «Огонь» | `fireRoutine()` | Те же маски 8×16, что в OS (§3.1), но с багом развёртки — см. §4.3. |
| — | «Дыхание» яркости | `brightnessRoutine()` | Плавное изменение яркости ±2/тик в пределах 1..globalBrightness (таймер проверяется внутри функции). |
| — | Смена цвета (рисунка) | `colorsRoutine()` | Перекраска зажжённых пикселей, `hue += 2` за тик. |
| — | Радуга рисунка | `rainbowColorsRoutine()` | Радуга по вертикальным полосам только на зажжённых пикселях. |
| — | Бегущая строка | `fillString()` | Текст шрифтом 3×5/5×7 (case 11 в цикле недостижим: MODES_AMOUNT=11 → автопрокрутка 0..10, строка только вручную — интерпретация исследователя). |

Игры BT v1.6: Змейка, Тетрис, Лабиринт (файлы `snake.ino`, `tetris.ino`, `maze.ino`).

### 4.3. Уникальные/отличающиеся алгоритмы BT v1.6 (выдержки)

**Синусоиды (wavesRoutine) — единственный эффект, которого нет в GyverMatrixOS.** Источник: https://raw.githubusercontent.com/AlexGyver/GyverMatrixBT/master/firmware/%D0%A1%D1%82%D0%B0%D1%80%D1%8B%D0%B5%20%D0%B2%D0%B5%D1%80%D1%81%D0%B8%D0%B8/GyverMatrixBT_v1.6/effects.ino

```cpp
// эффект "синусоиды"
#define WAVES_AMOUNT 2    // количество синусоид

// ******************************** СИНУСОИДЫ *******************************
#define DEG_TO_RAD 0.017453
int t;
byte w[WAVES_AMOUNT];
byte phi[WAVES_AMOUNT];
byte A[WAVES_AMOUNT];
CRGB waveColors[WAVES_AMOUNT];

void wavesRoutine() {
  if (loadingFlag) {
    loadingFlag = false;
    for (byte j = 0; j < WAVES_AMOUNT; j++) {
      // забиваем случайными данными
      w[j] = random(17, 25);
      phi[j] = random(0, 360);
      A[j] = HEIGHT / 2 * random(4, 11) / 10;
      waveColors[j] = CHSV(random(0, 9) * 28, 255, 255);
    }
  }
  if (effectTimer.isReady()) {

    // сдвигаем все пиксели вправо
    for (int i = WIDTH - 1; i > 0; i--)
      for (int j = 0; j < HEIGHT; j++)
        drawPixelXY(i, j, getPixColorXY(i - 1, j));

    // увеличиваем "угол"
    t++;
    if (t > 360) t = 0;

    // заливаем чёрным левую линию
    for (byte i = 0; i < HEIGHT; i++) {
      drawPixelXY(0, i, 0x000000);
    }

    // генерируем позицию точки через синус
    for (byte j = 0; j < WAVES_AMOUNT; j++) {
      float value = HEIGHT / 2 + (float)A[j] * sin((float)w[j] * t * DEG_TO_RAD + (float)phi[j] * DEG_TO_RAD);
      leds[getPixelNumber(0, (byte)value)] = waveColors[j];
    }
  }
}
```

Параметры: 2 синусоиды; частота `w` = 17..24 «градусов»/тик, фаза 0..359°, амплитуда `A = HEIGHT/2 * (0.4..1.0)`; кадр целиком сдвигается вправо, в левый столбец ставится точка `y = HEIGHT/2 + A·sin(w·t + φ)`, цвет — 9 дискретных оттенков. Порт на no-float: таблица синуса 0..359° (u8) или CORDIC — интерпретация исследователя.

**Отскок шариков от нарисованных препятствий (DRAW_WALLS) — только в BT.** Источник: тот же `effects.ino` v1.6:

```cpp
#define DRAW_WALLS 1      // режим с рисованием препятствий для шаров
// ...
      // отскок от нарисованных препятствий
      if (DRAW_WALLS) {
        uint32_t thisColor = getPixColorXY(coord[j][0] / 10 + 1, coord[j][1] / 10);
        if (thisColor == globalColor/* && vector[j][0] > 0*/) {
          vector[j][0] = -vector[j][0];
        }
        thisColor = getPixColorXY(coord[j][0] / 10 - 1, coord[j][1] / 10);
        if (thisColor == globalColor/* && vector[j][0] < 0*/) {
          vector[j][0] = -vector[j][0];
        }
        thisColor = getPixColorXY(coord[j][0] / 10, coord[j][1] / 10 + 1);
        if (thisColor == globalColor/* && vector[j][1] > 0*/) {
          vector[j][1] = -vector[j][1];
        }
        thisColor = getPixColorXY(coord[j][0] / 10, coord[j][1] / 10 - 1);
        if (thisColor == globalColor/* && vector[j][1] < 0*/) {
          vector[j][1] = -vector[j][1];
        }
      }
```

Шар отскакивает от клетки, покрашенной цветом рисования `globalColor` (пользователь рисует «стенки» с телефона). В GyverMatrixOS v1.15 блок отсутствует.

**Старый float-фейдер (в OS заменён на fadeToBlackBy).** Источник: тот же `effects.ino` v1.6:

```cpp
void fadePixel(byte i, byte j, byte step) {
  // измеряяем цвет текущего пикселя
  uint32_t thisColor = getPixColorXY(i, j);
  if (thisColor == 0) return;

  // разбиваем цвет на составляющие RGB
  byte rgb[3];
  rgb[0] = (thisColor >> 16) & 0xff;
  rgb[1] = (thisColor >> 8) & 0xff;
  rgb[2] = thisColor & 0xff;

  // ищем максимум
  byte maximum = max(max(rgb[0], rgb[1]), rgb[2]);
  float coef = 0;

  // если есть возможность уменьшить
  if (maximum >= step)
    // уменьшаем и находим коэффициент уменьшения
    coef = (float)(maximum - step) / maximum;

  // далее все цвета умножаем на этот коэффициент
  for (byte i = 0; i < 3; i++) {
    if (rgb[i] > 0) rgb[i] = (float)rgb[i] * coef;
    else rgb[i] = 0;
  }
  leds[getPixelNumber(i, j)] = CRGB(rgb[0], rgb[1], rgb[2]);
}
```

Яркость уменьшается на `step` по максимальному каналу с пропорциональным умножением всех каналов (float). В OS v1.15 тот же механизм переписан на `fadeToBlackBy` с отсечкой при каналах <30 (§3.3) — портить лучше OS-вариант (целочисленный).

**Баг огня в v1.6 (исправлен в OS).** Источники: `effects.ino` v1.6 и комментарий в `effects.ino` v1.15 (§3.1):

```cpp
// v1.6, shiftUp() и drawFrame():
      uint8_t newX = x;
      if (x > 15) newX = x - 15;     // ОШИБКА: должно быть x % 16
// ...
    leds[getPixelNumber(newX, 0)] = color;   // v1.6: пишется newX вместо x
```

В v1.6 нижняя строка огня пишется в `getPixelNumber(newX, 0)` — на матрицах шире 16 правый угол не работает; в OS v1.12+ исправлено на `x` (и `x % 16`), с комментарием про форум.

### 4.4. Чем набор BT v1.6 отличается от GyverMatrixOS v1.15

Сводка (все пункты проверены по коду, источники выше):

| Аспект | GyverMatrixBT v1.6 | GyverMatrixOS v1.15 |
|---|---|---|
| Эффекты в демо-цикле | 8 (снег, матрица, звездопад, кубик, шарики, **синусоиды**, радуга, огонь) | 19 эффектов + часы/лампа/рассвет/5 анимаций (EFFECT_LIST §2.1) |
| Шумовые эффекты (9 шт.) | нет | есть (`noise_effects.ino`) |
| Конфетти | нет | есть (`sparklesRoutine`) |
| Синусоиды («Линии») | **есть** | **удалены** |
| Шарики с препятствиями (DRAW_WALLS) | **есть** | удалено |
| Плотности/хвосты | захардкожены (snow 10, starfall 60, TRACK_STEP 40, TAIL_STEP 30) | вынесены в `#define` (SNOW_DENSE, STAR_DENSE, TRACK_STEP 70, TAIL_STEP 100) |
| Фейдер хвостов | float-коэффициент по максимуму канала | `fadeToBlackBy` (целочисленный) |
| Огонь | баг развёртки `newX = x - 15` и запись `getPixelNumber(newX, 0)` | исправлено (`x % 16`, `getPixelNumber(x, 0)`) |
| Часы/наложение, будильник-рассвет, гифки | нет | есть |
| Игры | 3 (змейка, тетрис, лабиринт) | 6 (+ Runner, Flappy Bird, Арканоид) |
| Связь | Bluetooth HC-06 (Serial 9600) | WiFi/UDP (ESP8266) + BT_MODE в v1.12 |
| Таймеры | `GTimer_ms` (GyverTimer.h), проверки `isReady()` внутри каждой routine | `timerMinim`, таймеры вынесены в `customRoutine`/`doEffectWithOverlay` |
| Скорость эффектов | одна на всё: `D_EFFECT_SPEED 100` мс, `globalSpeed` | индивидуальная per-эффект из EEPROM, дефолт 80 мс |
| Яркость по умолчанию | `BRIGHTNESS 60` | `BRIGHTNESS 32` (v1.12: 150) |

Важно: 7 из 8 эффектов BT (всё, кроме синусоид) алгоритмически идентичны OS-версиям — для порта достаточно реализовать OS-варианты (§3) с BT-параметрами там, где нужен более «медленный/длиннохвостый» вид. Исключения — `wavesRoutine`, `DRAW_WALLS` и баг огня (§4.3).

---

## 5. Что лучше подходит для «матрицы в окне» (вид со стороны улицы)

Этот раздел — интерпретация исследователя на основе алгоритмов (§3–4); в исходниках рекомендаций «для окна» нет. Единственное прямое свидетельство по теме — на странице проекта есть раздел «КРЕПЛЕНИЕ НА ОКНО» со съёмным креплением для матрицы (https://alexgyver.ru/gyvermatrixbt/), т.е. сценарий «матрица в окне» самим автором предусмотрен.

Принцип: с улицы матрица видна издалека и, как правило, зеркально. Значит нужны: крупные контрастные пятна, медленная читаемая динамика, минимум мелких деталей; и **зеркалирование по X** в нашем конфиге матрицы (в прошивках AlexGyver runtime-зеркала нет — см. §6.5; для «текстурных» эффектов зеркало не важно, для часов/текста/спрайтов — критично).

Рекомендуемый шорт-лист (для 8×8, ночь, окно):

1. **Огонь (fireRoutine)** — лучший эффект «на окно»: яркое тёплое пятно в нижней части («камин в окне»), максимум силуэтности; `HUE_ADD` позволяет перекрасить всё пламя. Порт: один факел маски 8×16 → 8×8, integer-интерполяция.
2. **Снегопад (snowRoutine)** — крупные белые хлопья, предельно читается с улицы и тематичен; для «крупного» снега на 8×8 уменьшить плотность (увеличить SNOW_DENSE с 10 до ~20–30 — интерпретация).
3. **The Matrix (matrixRoutine)** — высококонтрастные зелёные «капли» с хвостами; очень узнаваемый силуэт.
4. **Звездопад (starfallRoutine)** — диагональные кометы с хвостами, живая динамика; на 8×8 хвосты сольются в сплошные линии — эффект даже выигрышнее, чем на 16×16 (интерпретация).
5. **Радуга диагональная (rainbowDiagonalRoutine)** — сплошная заливка с движущимся градиентом: максимальная «читаемость пятна» с большого расстояния, без мелких деталей.
6. **Лава / Облака (lavaNoise/cloudNoise)** — шумовые пятна с большими «кляксами» (scale 50 и 30; в комментарии прошивки: «чем меньше scale, тем крупнее»); мягкая крупная фактура, отлично видна с улицы.
7. **Шарики (ballsRoutine) с хвостами (BALL_TRACK)** — движение и цвет; на 8×8 три шара с хвостами дают цельную картинку (BT-режим DRAW_WALLS для окна избыточен — это интерактив с телефона).
8. **Часы с наложением (OS v1.13+)** — практичное дополнение поверх любого фона (в BT-версии часов нет).
9. **Анимации/картинки (OS `animation()` + спрайты)** — крупные силуэты (в репозитории есть готовые 8×8-спрайты, например `Sprites/8x8/Superheroes` — https://github.com/AlexGyver/GyverMatrixBT/tree/master/firmware/GyverMatrixOS_v1.12/Sprites): для «новогоднего окна» — свои силуэты (снежинка, ель, Дед Мороз) в том же формате.
10. **Синусоиды (wavesRoutine, только BT)** — плавные цветные волны через всё окно; неплохой «спокойный» режим, но требует таблицы синуса для no-float порта.

Что **не** стоит портить первым для окна: Конфетти и Цветной шум — мелкозернистые, с расстояния превращаются в кашу (интерпретация); кубик без хвоста — слишком мелкий одиночный объект.

---

## 6. Технические допущения прошивок

### 6.1. Размер матрицы и физика

- Дефолты обеих линеек — 16×16: BT v1.6 `WIDTH 16, HEIGHT 16, BRIGHTNESS 60`; OS v1.15 `WIDTH 16, HEIGHT 16, BRIGHTNESS 32, CURRENT_LIMIT 1800, COLOR_ORDER GRB, SEGMENTS 1`; OS v1.12 `BRIGHTNESS 150, CURRENT_LIMIT 2000` (шапки соответствующих .ino).
- Гайд: на Nano 16×16 — впритык; Mega — до ~1700 диодов; физический предел WS2812: 500 диодов → 60 fps, 1000 → 30, 2000 → 15 (https://alexgyver.ru/gyvermatrixos-guide/).
- Эффекты параметричны по `WIDTH`/`HEIGHT`, **кроме огня**: маски `valueMask`/`hueMask` жёстко 8×16 (два «факела»; на 8×8 — один факел, интерпретация).

### 6.2. 1D или 2D

Все эффекты обеих прошивок — **2D** (`drawPixelXY(x,y)`/`getPixColorXY`, циклы WIDTH×HEIGHT). Чисто 1D-эффектов нет (они в GyverLamp). На 8×8 портируются напрямую с пересчётом плотностей/количеств.

### 6.3. FPS и тайминг

- Единого FPS нет: мягкие таймеры на режим. BT v1.6: `GTimer_ms effectTimer(D_EFFECT_SPEED=100 мс)`, проверка `isReady()` внутри каждой routine. OS v1.15: `timerMinim effectTimer(D_EFFECT_SPEED=80 мс)`, `FastLED.show()` из `doEffectWithOverlay()`; индивидуальные скорости per-эффект в EEPROM (`getEffectSpeed()`); текст 100 мс, игры 60 мс, точки часов 500 мс; автосмена режимов 30 с (v1.15) / 10 с (BT v1.6, OS v1.12).
- Эффект — **функция-шаг без параметра времени**: один вызов = один шаг на фиксированный интервал. Для нашего `elapsed_ms`: `tick = elapsed_ms / interval` с накоплением состояния шага (интерпретация исследователя).

### 6.4. Глобальные переменные, от которых зависят эффекты

Имена `thisBright`/`thisDelay` из постановки — глобалы **GyverLamp**, в этих прошивках их нет. Аналоги (проверено по исходникам):

| Переменная (файл) | Роль |
|---|---|
| `byte hue` (effects.ino) | оттенок для радуг/смены цвета; +1..+4 за тик |
| `int8_t thisMode` / `byte thisMode` (custom.ino) | индекс текущего режима демо-цикла |
| `boolean loadingFlag` | одноразовая инициализация режима (очистка, стартовые координаты) — аналог `Effect::init` |
| `int globalBrightness` / `byte globalBrightness` | общая яркость; «Дыхание» модулирует через `breathBrightness` |
| `int effectSpeed` (OS) / `globalSpeed` (BT) | интервал тика, мс |
| `uint32_t globalColor` | цвет рисования; в BT v1.6 — ещё и цвет «стен» для DRAW_WALLS |
| `CRGB leds[NUM_LEDS]` | глобальный фрейм; эффекты пишут прямо в него и читают соседей `getPixColorXY` |

Хелперы (utility.ino соответствующей версии): `drawPixelXY` (с отсечением границ), `getPixColor`/`getPixColorXY` (цвет как `uint32_t` 0xRRGGBB — на этом держатся арифметические фейды Matrix/снега), `getPixelNumber` (развёртка), `fillAll`, `gammaCorrection` (LUT γ≈2.8 в PROGMEM — только OS v1.15), `fader`/`fadePixel`.

### 6.5. Направление и зеркалирование

Задаётся **на этапе компиляции** (одинаково в обеих линейках):

```cpp
#define MATRIX_TYPE 0         // тип матрицы: 0 - зигзаг, 1 - параллельная
#define CONNECTION_ANGLE 0    // угол подключения: 0 - левый нижний, 1 - левый верхний, 2 - правый верхний, 3 - правый нижний
#define STRIP_DIRECTION 0     // направление ленты из угла: 0 - вправо, 1 - вверх, 2 - влево, 3 - вниз
```

Развёртка — препроцессорные макросы `THIS_X`/`THIS_Y`/`_WIDTH` (8 валидных комбинаций, иначе `#pragma message "Wrong matrix parameters! Set to default"`) и зигзаг в `getPixelNumber` (utility.ino):

```cpp
uint16_t getPixelNumber(int8_t x, int8_t y) {
  if ((THIS_Y % 2 == 0) || MATRIX_TYPE) {               // если чётная строка
    return (THIS_Y * _WIDTH + THIS_X);
  } else {                                              // если нечётная строка
    return (THIS_Y * _WIDTH + _WIDTH - THIS_X - 1);
  }
}
```

Runtime-зеркалирования/поворота нет. Для «окна» (вид с улицы) горизонтальное зеркало должен обеспечить наш конфиг матрицы (интерпретация исследователя).

### 6.6. Детерминированность и математика

- **Случайность:** везде Arduino `random()` с сидом `randomSeed(analogRead(...))` — прошивки недетерминированы между включениями. `random(a,b)` — верхняя граница исключена. Для детерминированных кадров garland-rs нужен собственный сидируемый PRNG (интерпретация).
- **Float:** в OS — интерполяция огня и радуга диагональная (§3.1, §3.7); в BT v1.6 — дополнительно `wavesRoutine` (`sin()`) и `fadePixel` (float-коэффициент) (§4.3). Все места переводятся в целочисленную арифметику/таблицы.
- **Цвет:** `CHSV` ↔ `uint32_t` 0xRRGGBB; HSV→RGB делает FastLED (vvip-68 требует FastLED 3.2.9+, https://github.com/vvip-68/GyverMatrixWiFi, раздел «Важно»); для совпадения палитр у нас нужен эквивалент `hsv2rgb_rainbow` (интерпретация).

---

## 7. Противоречия

1. **Примерный список из постановки vs реальность прошивок.** «Мета-шары», «светлячки», «шторм» в GyverMatrixOS и GyverMatrixBT отсутствуют (проверено по `EFFECT_LIST` v1.15 и `custom.ino` обеих версий); «змейка» — игра, не эффект. Ближайшие аналоги: «Шарики», «Звездопад», «Конфетти». Шторм/светлячки — из GyverLamp (https://github.com/AlexGyver/GyverLamp), отдельное исследование при необходимости.
2. **README GyverMatrixBT vs код v1.6.** README (раздел «Эффекты») не упоминает Звездопад, но `starfallRoutine()` есть в демо-цикле v1.6 (case 2, custom.ino). Имена «Летающие частицы»/«Линии» в README соответствуют `ballsRoutine`/`wavesRoutine` лишь по смыслу — в коде они называются «шарики»/«синусоиды».
3. **Комментарий vs код в звездопаде** (обе версии): комментарий «заполняем головами комет левую и верхнюю линию», фактически — левый край (нижняя половина) и нижняя строка (левая половина) — §3.4.
4. **Баг огня в BT v1.6** (`newX = x - 15` вместо `x % 16`, запись `getPixelNumber(newX, 0)`) — исправлен в OS, подтверждено комментарием в v1.15 — §4.3.
5. **Дефолты яркости расходятся:** README GyverMatrixBT — `BRIGHTNESS 60`; код v1.6 — 60 (совпадает), код OS v1.12 — 150, OS v1.15 — 32.
6. **README vvip-68 vs код v1.15:** README https://github.com/vvip-68/GyverMatrixWiFi обещает «Пейнтбол» и «Водоворот», в `EFFECT_LIST`/`customModes()` v1.15 их нет.
7. **Лицензия продолжения:** оба репозитория AlexGyver — MIT © 2020 AlexGyver, но в корне https://github.com/vvip-68/GyverMatrixWiFi файла LICENSE нет (прямой запрос LICENSE → 404, файла нет в листинге), хотя код v1.13–v1.15 написан vvip-68 («Дальнейшее развитие: vvip, 2019» в шапке файлов). Для частного проекта риск минимален; при публикации опираться на MIT-версии из репозиториев AlexGyver с указанием обоих авторов.

## 8. Missing evidence / что не проверено

- Тексты эффектов в Android-приложениях (имена могли отличаться от `EFFECT_LIST`/README) — APK не разбирался; имена взяты из прошивок.
- Алгоритмы игр (`snake/tetris/maze`, `g_*.ino`) не извлекались — в скоупе эффекты.
- Точное соответствие HSV→RGB FastLED (`hsv2rgb_rainbow`) нашей будущей реализации не сверялось — задача порта.
- Раздел «Настройки режимов» гайда подгружается скриптами и в извлечённом тексте виден не полностью; параметры взяты из первоисточников (`effects.ino`/`noise_effects.ino`), пробел некритичен.

## 9. Sources

Kept:
- Гайд GyverMatrixOS — https://alexgyver.ru/gyvermatrixos-guide/ (настройки, ограничения плат, FPS-пределы, анонс продолжения vvip-68)
- Страница проекта BT-матрицы — https://alexgyver.ru/gyvermatrixbt/ (обе линейки, крепление на окно, ссылка на vvip-68)
- AlexGyver/GyverMatrixBT — https://github.com/AlexGyver/GyverMatrixBT (репозиторий из шапки скетчей; GyverMatrixOS_v1.12 + «Старые версии» с GyverMatrixBT_v1.5/v1.6; README; LICENSE)
- LICENSE (дословный текст) — https://raw.githubusercontent.com/AlexGyver/GyverMatrixBT/master/LICENSE и https://raw.githubusercontent.com/AlexGyver/GyverMatrixWiFi/master/LICENSE
- GyverMatrixOS v1.15: effects.ino — https://raw.githubusercontent.com/AlexGyver/GyverMatrixWiFi/master/firmware/GyverMatrixOS_v1.15/effects.ino ; noise_effects.ino — https://raw.githubusercontent.com/AlexGyver/GyverMatrixWiFi/master/firmware/GyverMatrixOS_v1.15/noise_effects.ino ; GyverMatrixOS_v1.15.ino — https://raw.githubusercontent.com/AlexGyver/GyverMatrixWiFi/master/firmware/GyverMatrixOS_v1.15/GyverMatrixOS_v1.15.ino ; utility.ino — https://raw.githubusercontent.com/AlexGyver/GyverMatrixWiFi/master/firmware/GyverMatrixOS_v1.15/utility.ino ; custom.ino — https://raw.githubusercontent.com/AlexGyver/GyverMatrixWiFi/master/firmware/GyverMatrixOS_v1.15/custom.ino
- GyverMatrixOS v1.12 — https://raw.githubusercontent.com/AlexGyver/GyverMatrixBT/master/firmware/GyverMatrixOS_v1.12/GyverMatrixOS_v1.12.ino (сравнение версий)
- GyverMatrixBT v1.6: GyverMatrixBT_v1.6.ino — https://raw.githubusercontent.com/AlexGyver/GyverMatrixBT/master/firmware/%D0%A1%D1%82%D0%B0%D1%80%D1%8B%D0%B5%20%D0%B2%D0%B5%D1%80%D1%81%D0%B8%D0%B8/GyverMatrixBT_v1.6/GyverMatrixBT_v1.6.ino ; effects.ino — https://raw.githubusercontent.com/AlexGyver/GyverMatrixBT/master/firmware/%D0%A1%D1%82%D0%B0%D1%80%D1%8B%D0%B5%20%D0%B2%D0%B5%D1%80%D1%81%D0%B8%D0%B8/GyverMatrixBT_v1.6/effects.ino ; custom.ino — https://raw.githubusercontent.com/AlexGyver/GyverMatrixBT/master/firmware/%D0%A1%D1%82%D0%B0%D1%80%D1%8B%D0%B5%20%D0%B2%D0%B5%D1%80%D1%81%D0%B8%D0%B8/GyverMatrixBT_v1.6/custom.ino
- vvip-68/GyverMatrixWiFi — https://github.com/vvip-68/GyverMatrixWiFi (оригинал продолжения, статус «устарел», FastLED 3.2.9+)
- vvip-68/GyverPanelWiFi — https://github.com/vvip-68/GyverPanelWiFi (актуальный проект-продолжение)

Rejected/deprioritized:
- Темы forum.alexgyver.ru и Wokwi-копии скетча (wokwi.com/projects/385564428287784961, 411454024205571073) — вторичные копии, не первоисточник.
- AlexGyver/GyverLamp — другая прошивка (лампа); упомянута только как направление для «шторма/светлячков».

## 10. Next steps

1. Мини-исследование GyverLamp, если нужны «шторм», «светлячки», «мета-боллы» (там `beat88`/`sin8`/`random8`).
2. Протопорты в порядке возрастания сложности: Конфетти → Шарик → Шарики → The Matrix → Снегопад → Звездопад → Огонь (integer-интерполяция + половина маски) → Синусоиды (таблица синуса) → палитровые шумы (`inoise8`-эквивалент + палитры).
3. Зафиксировать целочисленные эквиваленты FastLED-примитивов (qadd8/qsub8/scale8/dim8_raw/fadeToBlackBy/ColorFromPalette + палитры) как общую мини-библиотеку garland-rs до портирования эффектов.
4. Решение по зеркалу: добавить `flip_x` в конфиг матрицы garland-rs для сценария «вид с улицы» (влияет на часы/текст/спрайты).
