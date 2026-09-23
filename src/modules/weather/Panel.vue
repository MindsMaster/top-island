<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { currentDate, currentTime } from '@/core/clock';
import { useI18n } from '@/core/i18n';
import { shellView } from '@/shell/view';
import {
  desc,
  hint,
  icon,
  iconFor,
  weatherState as snap,
  weatherView as view,
  type WeatherView,
} from './store';

/** 与 weather.scss 的 .wx 内容宽度一致 */
const CURVE_W = 340;
const CURVE_H = 80;
const NOWCAST_H = 26;
const STRIP_HOURS = 5;

const { t, weekdays } = useI18n();

const openDay = ref(-1);
const lastView = ref<WeatherView>('today');

function open(v: WeatherView) {
  openDay.value = -1;
  lastView.value = v;
  view.value = v;
}

function close() {
  view.value = null;
}

watch(
  () => shellView.panel,
  (p) => {
    if (p !== 'weather') close();
  }
);

const stripHours = computed(() => snap.hourly.slice(0, STRIP_HOURS));

function smooth(pts: Array<[number, number]>): string {
  let d = `M${pts[0][0]} ${pts[0][1]}`;
  for (let i = 0; i < pts.length - 1; i++) {
    const p0 = pts[i - 1] ?? pts[i];
    const [p1, p2] = [pts[i], pts[i + 1]];
    const p3 = pts[i + 2] ?? p2;
    d += ` C${p1[0] + (p2[0] - p0[0]) / 6} ${p1[1] + (p2[1] - p0[1]) / 6} ${p2[0] - (p3[0] - p1[0]) / 6} ${p2[1] - (p3[1] - p1[1]) / 6} ${p2[0]} ${p2[1]}`;
  }
  return d;
}

/** 现在 加之后每两小时一格 */
const curve = computed(() => {
  if (snap.temp === null) return null;
  const cols = [
    { label: t('weatherNow'), temp: snap.temp, icon: icon.value, pop: 0 },
    ...snap.hourly
      .filter((_, i) => i % 2 === 1)
      .map((h) => ({
        label: t('weatherHour', h.hour),
        temp: h.temp,
        icon: iconFor(h.kind, h.night),
        pop: h.pop,
      })),
  ];
  if (cols.length < 2) return null;
  const temps = cols.map((c) => c.temp);
  const lo = Math.min(...temps);
  const hi = Math.max(...temps);
  const cw = CURVE_W / cols.length;
  const pts = cols.map((c, i): [number, number] => [
    cw * (i + 0.5),
    CURVE_H - 10 - (hi === lo ? 0.5 : (c.temp - lo) / (hi - lo)) * (CURVE_H - 30),
  ]);
  const line = smooth(pts);
  return {
    cols: cols.map((c, i) => ({ ...c, x: pts[i][0], y: pts[i][1] })),
    line,
    area: `${line} L${pts[pts.length - 1][0]} ${CURVE_H} L${pts[0][0]} ${CURVE_H}Z`,
  };
});

const nowcastBars = computed(() => {
  const n = snap.nowcast;
  if (!n || !n.values.some((v) => v > 0)) return null;
  const max = Math.max(...n.values);
  const bw = CURVE_W / n.values.length;
  const hours = Math.round((n.values.length * n.stepMin) / 60);
  return {
    bars: n.values.map((v, i) => {
      const h = Math.max((v / max) * NOWCAST_H, 1.5);
      return { x: i * bw + 1, y: NOWCAST_H - h, w: bw - 2, h, wet: v > 0 };
    }),
    ticks: Array.from({ length: hours + 1 }, (_, i) => (i ? t('weatherInHours', i) : t('weatherNow'))),
  };
});

const TEMP_STOPS: Array<[number, [number, number, number]]> = [
  [-10, [150, 180, 255]],
  [5, [120, 205, 225]],
  [18, [150, 220, 150]],
  [26, [245, 205, 100]],
  [34, [255, 140, 80]],
];

function tempColor(v: number): string {
  let [t0, c0] = TEMP_STOPS[0];
  if (v <= t0) return `rgb(${c0})`;
  for (const [t1, c1] of TEMP_STOPS.slice(1)) {
    if (v <= t1) {
      const f = (v - t0) / (t1 - t0);
      return `rgb(${c0.map((x, j) => Math.round(x + (c1[j] - x) * f))})`;
    }
    [t0, c0] = [t1, c1];
  }
  return `rgb(${c0})`;
}

const week = computed(() => {
  const days = snap.daily;
  if (!days.length) return [];
  const lo = Math.min(...days.map((d) => d.lo));
  const span = Math.max(...days.map((d) => d.hi)) - lo || 1;
  const pos = (v: number) => ((v - lo) / span) * 100;
  return days.map((d, i) => ({
    ...d,
    name:
      i === 0
        ? t('weatherToday')
        : i === 1
          ? t('weatherTomorrow')
          : t('weatherWeekday', weekdays.value[new Date(d.time).getDay()]),
    icon: iconFor(d.kind, false),
    barStyle: {
      left: `${pos(d.lo)}%`,
      right: `${100 - pos(d.hi)}%`,
      background: `linear-gradient(90deg, ${tempColor(d.lo)}, ${tempColor(d.hi)})`,
    },
    nowLeft: i === 0 && snap.temp !== null ? `${pos(Math.min(Math.max(snap.temp, d.lo), d.hi))}%` : null,
  }));
});
</script>

<template>
  <div class="wx" :class="{ 'wx-in-detail': view }">
    <div class="wx-home" :inert="!!view">
      <div class="wx-top">
        <div class="wx-city">
          <i class="fa-solid fa-location-dot"></i>
          <span>{{ snap.city || '--' }}</span>
        </div>
        <div class="wx-clock">
          <div class="wx-time">{{ currentTime || '--:--' }}</div>
          <div class="wx-date">{{ currentDate }}</div>
        </div>
      </div>

      <div class="wx-hero">
        <div class="wx-temp">{{ snap.temp ?? '--' }}<span class="wx-deg">°</span></div>
        <div class="wx-side">
          <div class="wx-desc">
            <i :class="'fa-solid ' + icon"></i>
            <span>{{ snap.error || desc }}</span>
          </div>
          <div v-if="snap.tempHi !== null" class="wx-sub">
            <template v-if="snap.feels !== null">{{ t('weatherFeels') }} {{ snap.feels }}° · </template>
            {{ snap.tempHi }}° / {{ snap.tempLo }}°
          </div>
        </div>
      </div>

      <button v-if="stripHours.length" class="wx-card" @click="open(lastView)">
        <span class="wx-card-head">
          <span class="wx-card-title">{{ hint ?? t('weatherHourly') }}</span>
          <span class="wx-card-more">{{ t('weatherDetails') }}<i class="fa-solid fa-chevron-right"></i></span>
        </span>
        <span class="wx-strip">
          <span class="wx-hour">
            <span class="wx-hour-label">{{ t('weatherNow') }}</span>
            <i :class="'fa-solid ' + icon"></i>
            <span class="wx-hour-temp">{{ snap.temp }}°</span>
            <span class="wx-hour-pop"></span>
          </span>
          <span v-for="h in stripHours" :key="h.time" class="wx-hour">
            <span class="wx-hour-label">{{ t('weatherHour', h.hour) }}</span>
            <i :class="'fa-solid ' + iconFor(h.kind, h.night)"></i>
            <span class="wx-hour-temp">{{ h.temp }}°</span>
            <span class="wx-hour-pop">{{ h.pop >= 20 ? `${h.pop}%` : '' }}</span>
          </span>
        </span>
      </button>
    </div>

    <div class="wx-detail" :inert="!view">
      <div class="wx-dhead">
        <button class="wx-back" :aria-label="t('weatherBack')" @click="close">
          <i class="fa-solid fa-chevron-left"></i>
        </button>
        <div class="wx-dnow">
          <i :class="'fa-solid ' + icon"></i>
          <span>{{ snap.temp ?? '--' }}° {{ desc }}</span>
        </div>
        <div class="wx-pill">
          <button :class="{ on: view === 'today' }" @click="open('today')">{{ t('weatherToday') }}</button>
          <button :class="{ on: view === 'week' }" @click="open('week')">{{ t('weatherWeek') }}</button>
        </div>
      </div>

      <div v-if="view === 'today'" key="today" class="wx-dbody">
        <div v-if="snap.nowcast" class="wx-nowcast">
          <div class="wx-nowcast-text">{{ snap.nowcast.summary }}</div>
          <template v-if="nowcastBars">
            <svg :viewBox="`0 0 ${CURVE_W} ${NOWCAST_H}`" :width="CURVE_W" :height="NOWCAST_H">
              <rect
                v-for="(b, i) in nowcastBars.bars"
                :key="i"
                :x="b.x"
                :y="b.y"
                :width="b.w"
                :height="b.h"
                rx="1.5"
                :class="b.wet ? 'wx-bar-wet' : 'wx-bar-dry'"
              />
            </svg>
            <div class="wx-ticks">
              <span v-for="tick in nowcastBars.ticks" :key="tick">{{ tick }}</span>
            </div>
          </template>
        </div>

        <div v-if="curve" class="wx-curve">
          <div class="wx-curve-cols" :style="{ gridTemplateColumns: `repeat(${curve.cols.length}, 1fr)` }">
            <div v-for="c in curve.cols" :key="c.label" class="wx-curve-col">
              <span>{{ c.label }}</span>
              <i :class="'fa-solid ' + c.icon"></i>
            </div>
          </div>
          <svg :viewBox="`0 0 ${CURVE_W} ${CURVE_H}`" :width="CURVE_W" :height="CURVE_H">
            <path :d="curve.area" class="wx-curve-area" />
            <path :d="curve.line" class="wx-curve-line" />
            <g v-for="(c, i) in curve.cols" :key="c.label">
              <circle :cx="c.x" :cy="c.y" :r="i ? 2 : 3" class="wx-curve-dot" />
              <text :x="c.x" :y="c.y - 7" text-anchor="middle" class="wx-curve-temp">{{ c.temp }}°</text>
            </g>
          </svg>
          <div class="wx-curve-cols" :style="{ gridTemplateColumns: `repeat(${curve.cols.length}, 1fr)` }">
            <span v-for="c in curve.cols" :key="c.label" class="wx-curve-pop">{{
              c.pop >= 20 ? `${c.pop}%` : ''
            }}</span>
          </div>
        </div>

        <div class="wx-stats">
          <div v-if="snap.feels !== null">
            <small>{{ t('weatherFeels') }}</small
            ><span>{{ snap.feels }}°</span>
          </div>
          <div v-if="snap.humidity !== null">
            <small>{{ t('weatherHumidity') }}</small
            ><span>{{ snap.humidity }}%</span>
          </div>
          <div v-if="snap.wind">
            <small>{{ t('weatherWind') }}</small
            ><span>{{ snap.wind }}</span>
          </div>
          <div v-if="snap.uv">
            <small>{{ t('weatherUv') }}</small
            ><span>{{ snap.uv }}</span>
          </div>
        </div>
        <div class="wx-foot">
          <template v-if="snap.daily[0]">
            <span>{{ t('weatherSunrise') }} {{ snap.daily[0].sunrise }}</span>
            <span>{{ t('weatherSunset') }} {{ snap.daily[0].sunset }}</span>
          </template>
          <span v-if="snap.visibility !== null">{{ t('weatherVisibility') }} {{ snap.visibility }} km</span>
          <span v-if="snap.aqi">{{ snap.aqi }}</span>
        </div>
      </div>

      <div v-else-if="view === 'week'" key="week" class="wx-dbody wx-days">
        <div v-for="(d, i) in week" :key="d.time" class="wx-day" :class="{ open: openDay === i }">
          <button class="wx-day-row" @click="openDay = openDay === i ? -1 : i">
            <span class="wx-day-name">{{ d.name }}</span>
            <i :class="'fa-solid ' + d.icon"></i>
            <span class="wx-day-pop">{{ d.pop >= 20 ? `${d.pop}%` : '' }}</span>
            <span class="wx-day-lo">{{ d.lo }}°</span>
            <span class="wx-day-bar">
              <i :style="d.barStyle"></i>
              <b v-if="d.nowLeft" :style="{ left: d.nowLeft }"></b>
            </span>
            <span class="wx-day-hi">{{ d.hi }}°</span>
          </button>
          <div v-if="openDay === i" class="wx-day-more">
            {{ d.capDay }} / {{ d.capNight }}
            <template v-if="d.rain > 0"> · {{ t('weatherRainAmount', d.rain.toFixed(1)) }}</template>
            · {{ t('weatherSunset') }} {{ d.sunset }}
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
