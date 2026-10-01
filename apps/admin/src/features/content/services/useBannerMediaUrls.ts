import { ref, watch, type Ref } from 'vue'
import type { ContentDraftV2 } from '@airtek/contracts'
import { getMediaAsset } from '@/features/media'

export function useBannerMediaUrls(draft: Readonly<Ref<ContentDraftV2 | null>>) {
  const urls = ref<Record<string, string>>({})
  watch(() => draft.value?.composition.blocks.flatMap(block => block.type === 'hero' && block.media
    ? [block.media.asset.assetId] : []) ?? [], async (ids) => {
    await Promise.all([...new Set(ids)].filter(id => !urls.value[id] && !id.startsWith('pending-media:'))
      .map(async id => {
        try {
          const asset = await getMediaAsset(id)
          if (asset.previewUrl || asset.publicUrl) urls.value[id] = asset.previewUrl ?? asset.publicUrl!
        } catch { /* The field reports asset errors; preview keeps its readable fallback. */ }
      }))
  }, { immediate: true })
  return urls
}
