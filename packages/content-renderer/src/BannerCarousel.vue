<script lang="ts">
export interface BannerSlide {
  id: string
  eyebrow: string
  heading: string
  lead: string
  image?: string
  alt: string
  actions: Array<{ label: string; href: string }>
}
</script>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'

const props = withDefaults(defineProps<{
  slides: BannerSlide[]
  preview?: boolean
}>(), { preview: false })

const carousel = ref<HTMLElement | null>(null)
const active = ref(0)
const paused = ref(false)
const hovered = ref(false)
const focused = ref(false)
const hidden = ref(false)
const reduced = ref(false)
const failedImages = ref<Record<string, string>>({})
const multiple = computed(() => props.slides.length > 1)
const playing = computed(() => multiple.value && !props.preview && !paused.value
  && !hovered.value && !focused.value && !hidden.value && !reduced.value)
let timer: ReturnType<typeof setInterval> | undefined
let motion: MediaQueryList | undefined
let touchStart: { x: number; y: number } | undefined

function go(index: number): void {
  const count = props.slides.length
  active.value = count ? (index + count) % count : 0
  restartTimer()
}

function restartTimer(): void {
  if (timer) clearInterval(timer)
  timer = playing.value ? setInterval(() => go(active.value + 1), 6000) : undefined
}

function visibility(): void { hidden.value = document.hidden }
function motionChange(): void { reduced.value = motion?.matches ?? false }
function focusOut(event: FocusEvent): void {
  focused.value = Boolean(event.relatedTarget && (event.currentTarget as HTMLElement).contains(event.relatedTarget as Node))
}
function keydown(event: KeyboardEvent): void {
  if (!multiple.value || !['ArrowLeft', 'ArrowRight'].includes(event.key)) return
  event.preventDefault()
  go(active.value + (event.key === 'ArrowRight' ? 1 : -1))
}
function startTouch(event: TouchEvent): void {
  const point = event.touches[0]
  touchStart = point ? { x: point.clientX, y: point.clientY } : undefined
}
function endTouch(event: TouchEvent): void {
  const point = event.changedTouches[0]
  if (multiple.value && point && touchStart) {
    const dx = point.clientX - touchStart.x
    const dy = point.clientY - touchStart.y
    if (Math.abs(dx) > 50 && Math.abs(dx) > Math.abs(dy)) go(active.value + (dx < 0 ? 1 : -1))
  }
  touchStart = undefined
}

watch(playing, restartTimer)
watch(() => props.slides.map(slide => slide.id).join(','), () => go(0))
onMounted(() => {
  // An SSR image can fail before hydration attaches its error listener.
  for (const image of carousel.value?.querySelectorAll<HTMLImageElement>('img[data-banner-id]') ?? []) {
    if (image.complete && image.naturalWidth === 0) {
      failedImages.value[image.dataset.bannerId!] = image.getAttribute('src') ?? ''
    }
  }
  motion = window.matchMedia('(prefers-reduced-motion: reduce)')
  motionChange()
  visibility()
  motion.addEventListener('change', motionChange)
  document.addEventListener('visibilitychange', visibility)
  restartTimer()
})
onBeforeUnmount(() => {
  if (timer) clearInterval(timer)
  motion?.removeEventListener('change', motionChange)
  document.removeEventListener('visibilitychange', visibility)
})
</script>

<template>
  <section v-if="slides.length" ref="carousel" class="banner-carousel" role="region"
    :aria-label="preview ? '首页 Banner 预览' : 'Homepage banner'" aria-roledescription="carousel"
    tabindex="0" @keydown="keydown" @mouseenter="hovered = true" @mouseleave="hovered = false"
    @focusin="focused = true" @focusout="focusOut" @touchstart.passive="startTouch" @touchend.passive="endTouch">
    <div class="banner-carousel__track" :style="{ transform: `translateX(-${active * 100}%)` }">
      <article v-for="(slide, index) in slides" :key="slide.id" class="banner-carousel__slide"
        :inert="index !== active ? true : undefined" :aria-hidden="index !== active ? 'true' : undefined"
        role="group" aria-roledescription="slide" :aria-label="`${index + 1} / ${slides.length}`">
        <img v-if="slide.image && failedImages[slide.id] !== slide.image" class="banner-carousel__image"
          :data-banner-id="slide.id" :src="slide.image" :alt="slide.alt" :loading="index === 0 ? 'eager' : 'lazy'"
          :fetchpriority="index === 0 ? 'high' : 'auto'" decoding="async"
          @error="failedImages[slide.id] = slide.image" />
        <div class="banner-carousel__gradient" aria-hidden="true" />
        <div class="banner-carousel__copy shell">
          <p v-if="slide.eyebrow" class="banner-carousel__eyebrow">{{ slide.eyebrow }}</p>
          <component :is="index === active ? 'h1' : 'h2'">{{ slide.heading }}</component>
          <p v-if="slide.lead" class="banner-carousel__lead">{{ slide.lead }}</p>
          <div v-if="slide.actions.length" class="banner-carousel__actions">
            <a v-for="(action, position) in slide.actions" :key="`${position}-${action.href}`"
              :href="action.href" @click="preview && $event.preventDefault()">{{ action.label }}</a>
          </div>
        </div>
      </article>
    </div>
    <div v-if="multiple" class="banner-carousel__controls">
      <button type="button" :aria-label="preview ? '上一页 Banner' : 'Previous banner'" @click="go(active - 1)">←</button>
      <button v-for="(slide, index) in slides" :key="slide.id" type="button"
        :aria-label="preview ? `显示 Banner ${index + 1}` : `Show banner ${index + 1}`"
        :aria-current="index === active ? 'true' : undefined" @click="go(index)">{{ index + 1 }}</button>
      <button type="button" :aria-label="preview ? '下一页 Banner' : 'Next banner'" @click="go(active + 1)">→</button>
      <button v-if="!preview" type="button" :aria-label="paused || reduced ? 'Play banners' : 'Pause banners'"
        @click="paused = !(paused || reduced); reduced = false">{{ paused || reduced ? '▶' : 'Ⅱ' }}</button>
      <span class="banner-carousel__status" :aria-live="playing ? 'off' : 'polite'">{{ active + 1 }} / {{ slides.length }}</span>
    </div>
  </section>
</template>

<style scoped>
@layer components {
  .banner-carousel { position: relative; overflow: hidden; background: #092e39; color: white; touch-action: pan-y; }
  .banner-carousel:focus-visible { outline: 3px solid var(--airtek-green); outline-offset: -3px; }
  .banner-carousel__track { display: flex; align-items: stretch; transition: transform 450ms ease; }
  .banner-carousel__slide { position: relative; isolation: isolate; flex: 0 0 100%; min-width: 0; background: #092e39; }
  .banner-carousel__image { position: absolute; z-index: 0; inset: 0; width: 100%; height: 100%; object-fit: cover; }
  .banner-carousel__gradient { position: absolute; z-index: 1; inset: 0; background: linear-gradient(90deg, rgb(9 46 57 / 98%) 0%, rgb(9 46 57 / 92%) 45%, rgb(9 46 57 / 80%) 65%, rgb(9 46 57 / 0%) 100%); }
  .banner-carousel__copy { position: relative; z-index: 2; box-sizing: border-box; width: min(100% - 3rem, 1200px); margin-inline: auto; padding-block: clamp(3.5rem, 7vw, 7rem) 6rem; }
  .banner-carousel__eyebrow { margin: 0 0 1rem; color: white; font-weight: 800; letter-spacing: .12em; text-transform: uppercase; }
  .banner-carousel h1, .banner-carousel h2 { max-width: 15ch; margin: 0 0 1.4rem; color: white; font-size: clamp(2.5rem, 5vw, 4.8rem); line-height: 1.08; }
  .banner-carousel__lead { max-width: min(49rem, 65%); margin: 0; color: white; font-size: clamp(1.05rem, 1.7vw, 1.35rem); line-height: 1.7; }
  .banner-carousel__actions { display: flex; flex-wrap: wrap; gap: .75rem; margin-top: 1.75rem; }
  .banner-carousel__actions a { display: inline-flex; align-items: center; min-height: 44px; padding: .65rem 1.15rem; border: 1px solid white; border-radius: 6px; background: white; color: #092e39; font-weight: 700; text-decoration: none; }
  .banner-carousel__actions a + a { background: #092e39; color: white; }
  .banner-carousel__controls { position: absolute; z-index: 3; bottom: 1rem; left: 50%; display: flex; gap: .35rem; align-items: center; max-width: calc(100% - 2rem); overflow-x: auto; transform: translateX(-50%); }
  .banner-carousel__controls button { flex: 0 0 auto; width: 44px; height: 44px; padding: 0; border: 1px solid white; border-radius: 50%; background: #092e39; color: white; cursor: pointer; }
  .banner-carousel__controls button[aria-current='true'] { background: white; color: #092e39; }
  .banner-carousel button:focus-visible, .banner-carousel a:focus-visible { outline: 3px solid white; outline-offset: 3px; }
  .banner-carousel__status { position: absolute; width: 1px; height: 1px; overflow: hidden; clip-path: inset(50%); }
  @media (max-width: 700px) {
    .banner-carousel__gradient { background: linear-gradient(90deg, rgb(9 46 57 / 98%), rgb(9 46 57 / 85%)); }
    .banner-carousel__lead { max-width: 100%; }
  }
  @media (prefers-reduced-motion: reduce) { .banner-carousel__track { transition: none; } }
}
</style>
