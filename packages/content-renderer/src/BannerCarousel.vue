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
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import BannerIcon from './BannerIcon.vue'
import { useBannerTransition } from './useBannerTransition'
import './banner-carousel.css'

const props = withDefaults(defineProps<{
  slides: BannerSlide[]
  preview?: boolean
}>(), { preview: false })

const carousel = ref<HTMLElement | null>(null)
const dots = ref<HTMLElement | null>(null)
const paused = ref(false)
const hovered = ref(false)
const focused = ref(false)
const hidden = ref(false)
const reduced = ref(false)
const motionPlay = ref(false)
const playbackPaused = computed(() => paused.value || (reduced.value && !motionPlay.value))
const failedImages = ref<Record<string, string>>({})
const count = computed(() => props.slides.length)
const multiple = computed(() => count.value > 1)
const { active, outgoing, direction, phase, go: move, step: moveStep, reset, transitionEnd } = useBannerTransition(count, reduced)
const playing = computed(() => multiple.value && !props.preview && !paused.value
  && !hovered.value && !focused.value && !hidden.value && (!reduced.value || motionPlay.value))
let timer: ReturnType<typeof setInterval> | undefined
let motion: MediaQueryList | undefined
let touchStart: { x: number; y: number } | undefined

function go(index: number): void {
  move(index)
  restartTimer()
}
function step(movement: number): void {
  moveStep(movement)
  restartTimer()
}

function restartTimer(): void {
  if (timer) clearInterval(timer)
  timer = playing.value ? setInterval(() => step(1), 6000) : undefined
}

function visibility(): void { hidden.value = document.hidden }
function motionChange(): void { reduced.value = motion?.matches ?? false; motionPlay.value = false }
function togglePlayback(): void {
  paused.value = !playbackPaused.value
  if (reduced.value && !paused.value) motionPlay.value = true
}
function focusOut(event: FocusEvent): void {
  focused.value = Boolean(event.relatedTarget && (event.currentTarget as HTMLElement).contains(event.relatedTarget as Node))
}
function keydown(event: KeyboardEvent): void {
  if (!multiple.value || !['ArrowLeft', 'ArrowRight'].includes(event.key)) return
  event.preventDefault()
  step(event.key === 'ArrowRight' ? 1 : -1)
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
    if (Math.abs(dx) > 50 && Math.abs(dx) > Math.abs(dy)) step(dx < 0 ? 1 : -1)
  }
  touchStart = undefined
}

watch(playing, restartTimer)
watch(() => props.slides.map(slide => slide.id).join(','), () => { reset(); restartTimer() })
watch(active, async () => {
  await nextTick()
  const button = dots.value?.querySelector<HTMLElement>('[aria-current=true]')
  if (!button || !dots.value) return
  const left = button.offsetLeft
  dots.value.scrollLeft = Math.max(left - (dots.value.clientWidth - button.offsetWidth) / 2, 0)
})
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
    :class="{ 'is-preparing': phase === 'prepare', 'is-moving': phase === 'moving' }" :style="{ '--banner-direction': direction }"
    :aria-label="preview ? '首页 Banner 预览' : 'Homepage banner'" aria-roledescription="carousel"
    tabindex="0" @keydown="keydown" @mouseenter="hovered = true" @mouseleave="hovered = false"
    @focusin="focused = true" @focusout="focusOut" @touchstart.passive="startTouch" @touchend.passive="endTouch">
    <div class="banner-carousel__track">
      <article v-for="(slide, index) in slides" :key="slide.id" class="banner-carousel__slide"
        :class="{ 'is-active': index === active, 'is-outgoing': index === outgoing }" @transitionend="transitionEnd"
        :inert="index !== active ? true : undefined" :aria-hidden="index !== active ? 'true' : undefined"
        role="group" aria-roledescription="slide" :aria-label="`${index + 1} / ${slides.length}`">
        <img v-if="slide.image && failedImages[slide.id] !== slide.image" class="banner-carousel__image"
          :data-banner-id="slide.id" :src="slide.image" :alt="slide.alt" :loading="index === active || index === (active + 1) % count || index === (active + count - 1) % count ? 'eager' : 'lazy'"
          :fetchpriority="index === 0 ? 'high' : 'auto'" decoding="async"
          @error="failedImages[slide.id] = slide.image" />
        <div class="banner-carousel__gradient" aria-hidden="true" />
        <div class="banner-carousel__copy">
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
    <template v-if="multiple">
      <button class="banner-carousel__arrow banner-carousel__arrow--previous" type="button"
        :aria-label="preview ? '上一页 Banner' : 'Previous banner'" @click="step(-1)"><BannerIcon kind="previous" /></button>
      <button class="banner-carousel__arrow banner-carousel__arrow--next" type="button"
        :aria-label="preview ? '下一页 Banner' : 'Next banner'" @click="step(1)"><BannerIcon kind="next" /></button>
      <div class="banner-carousel__controls">
        <div ref="dots" class="banner-carousel__dots">
          <button v-for="(slide, index) in slides" :key="slide.id" class="banner-carousel__dot" type="button"
            :aria-label="preview ? `显示 Banner ${index + 1}` : `Show banner ${index + 1}`"
            :aria-current="index === active ? 'true' : undefined" @click="go(index)" />
        </div>
        <button v-if="!preview" class="banner-carousel__play" type="button"
          :aria-label="playbackPaused ? 'Play banners' : 'Pause banners'"
          @click="togglePlayback"><BannerIcon :kind="playbackPaused ? 'play' : 'pause'" /></button>
        <span class="banner-carousel__status" :aria-live="playing ? 'off' : 'polite'">{{ active + 1 }} / {{ slides.length }}</span>
      </div>
    </template>
  </section>
</template>
