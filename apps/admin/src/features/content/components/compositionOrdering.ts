import type { ContentBlock, ContentBlockKind } from '@airtek/contracts'

/** Block kinds the controlled template always fixes in place. */
export function requiredBlockSet(requiredBlocks: readonly ContentBlockKind[]): Set<ContentBlockKind> {
  return new Set(requiredBlocks)
}

export function isRequiredBlock(
  requiredKinds: ReadonlySet<ContentBlockKind>,
  block: ContentBlock,
): boolean {
  return requiredKinds.has(block.type)
}

/** Optional kinds the editor may append; required regions are never re-added. */
export function addableBlockKinds(
  allowedBlocks: readonly ContentBlockKind[],
  requiredBlocks: readonly ContentBlockKind[],
): ContentBlockKind[] {
  const required = requiredBlockSet(requiredBlocks)
  return allowedBlocks.filter((kind) => !required.has(kind))
}

/**
 * Required template regions stay fixed, so a swap is refused whenever either
 * side of the swap is required.
 */
export function canMoveBlock(
  blocks: readonly ContentBlock[],
  requiredBlocks: readonly ContentBlockKind[],
  blockId: string,
  offset: -1 | 1,
): boolean {
  const required = requiredBlockSet(requiredBlocks)
  const index = blocks.findIndex((entry) => entry.id === blockId)
  if (index < 0) return false
  const block = blocks[index]
  if (required.has(block.type)) return false
  const neighbour = blocks[index + offset]
  return Boolean(neighbour) && !required.has(neighbour.type)
}

export function moveBlock(
  blocks: readonly ContentBlock[],
  requiredBlocks: readonly ContentBlockKind[],
  blockId: string,
  offset: -1 | 1,
): ContentBlock[] | null {
  if (!canMoveBlock(blocks, requiredBlocks, blockId, offset)) return null
  const index = blocks.findIndex((entry) => entry.id === blockId)
  const target = index + offset
  const next = [...blocks]
  const current = next[index]
  next[index] = next[target]
  next[target] = current
  return next
}
