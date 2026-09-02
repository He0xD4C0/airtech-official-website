import { createHmac } from 'node:crypto'

function decodeBase32(value: string): Buffer {
  const alphabet = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567'
  let bits = ''
  for (const character of value.replace(/=+$/u, '').toUpperCase()) {
    const index = alphabet.indexOf(character)
    if (index < 0) throw new Error('The TOTP enrollment returned invalid Base32.')
    bits += index.toString(2).padStart(5, '0')
  }
  const bytes: number[] = []
  for (let offset = 0; offset + 8 <= bits.length; offset += 8) {
    bytes.push(Number.parseInt(bits.slice(offset, offset + 8), 2))
  }
  return Buffer.from(bytes)
}

export function totp(secret: string, epochMilliseconds = Date.now()): string {
  const counter = Math.floor(epochMilliseconds / 30_000)
  const message = Buffer.alloc(8)
  message.writeBigUInt64BE(BigInt(counter))
  const digest = createHmac('sha1', decodeBase32(secret)).update(message).digest()
  const offset = digest[digest.length - 1]! & 0x0f
  const binary = (digest.readUInt32BE(offset) & 0x7fff_ffff) % 1_000_000
  return binary.toString().padStart(6, '0')
}
