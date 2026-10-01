import { nextTick, onBeforeUnmount, ref, watch, type Ref } from 'vue'

/** Animate only the departing and arriving slides, regardless of their indices. */
export function useBannerTransition(count: Ref<number>, reduced: Ref<boolean>) {
  const active = ref(0)
  const outgoing = ref<number | null>(null)
  const direction = ref(1)
  const phase = ref<'idle' | 'prepare' | 'moving'>('idle')
  let queued: { index: number; direction: number } | undefined
  let frame: number | undefined
  let fallback: ReturnType<typeof setTimeout> | undefined
  let revision = 0
  let disposed = false

  function cancel(): void {
    revision += 1
    if (frame !== undefined) cancelAnimationFrame(frame)
    if (fallback) clearTimeout(fallback)
    frame = undefined
    fallback = undefined
  }

  function finish(): void {
    cancel()
    phase.value = 'idle'
    outgoing.value = null
    const target = queued
    queued = undefined
    if (target) void nextTick(() => go(target.index, target.direction))
  }

  function go(index: number, movement = index >= active.value ? 1 : -1): void {
    if (disposed) return
    const target = count.value ? ((index % count.value) + count.value) % count.value : 0
    if (phase.value !== 'idle') {
      queued = { index: target, direction: movement }
      return
    }
    if (target === active.value) return
    if (reduced.value) { active.value = target; return }
    direction.value = movement
    outgoing.value = active.value
    active.value = target
    phase.value = 'prepare'
    const currentRevision = ++revision
    void nextTick(() => {
      if (currentRevision !== revision) return
      frame = requestAnimationFrame(() => {
        frame = requestAnimationFrame(() => {
          if (currentRevision !== revision) return
          phase.value = 'moving'
          // Also settle when a browser suspends or omits transitionend.
          fallback = setTimeout(finish, 400)
        })
      })
    })
  }

  function step(movement: number): void {
    go((queued?.index ?? active.value) + movement, movement)
  }

  function transitionEnd(event: TransitionEvent): void {
    if (event.target === event.currentTarget && event.propertyName === 'transform' && phase.value === 'moving') finish()
  }

  function reset(): void {
    cancel()
    queued = undefined
    outgoing.value = null
    phase.value = 'idle'
    active.value = 0
  }

  watch(reduced, value => { if (value) finish() })
  onBeforeUnmount(() => { disposed = true; cancel() })
  return { active, outgoing, direction, phase, go, step, reset, transitionEnd }
}
