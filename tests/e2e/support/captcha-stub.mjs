// Acceptance-stack CAPTCHA provider stub.
//
// The API posts Turnstile-shaped siteverify forms to AIRTEK_CAPTCHA_ENDPOINT
// when the isolated stack enables the override. `pass` verifies, `reject` is a
// provider rejection, and `offline` answers with a non-JSON gateway error so
// the API treats the provider as unreachable and applies its failure mode.
import { createServer } from 'node:http'

const port = Number(process.env.CAPTCHA_STUB_PORT ?? 8098)

function readBody(request) {
  return new Promise((resolve) => {
    const chunks = []
    request.on('data', (chunk) => chunks.push(chunk))
    request.on('end', () => resolve(Buffer.concat(chunks).toString('utf8')))
  })
}

function json(response, status, payload) {
  const body = JSON.stringify(payload)
  response.writeHead(status, {
    'content-type': 'application/json',
    'content-length': Buffer.byteLength(body),
  })
  response.end(body)
}

const server = createServer(async (request, response) => {
  const url = new URL(request.url ?? '/', `http://${request.headers.host ?? 'stub'}`)
  if (request.method === 'POST' && url.pathname === '/siteverify') {
    const form = new URLSearchParams(await readBody(request))
    const token = form.get('response') ?? ''
    if (!form.get('secret')) {
      json(response, 200, { success: false, 'error-codes': ['missing-input-secret'] })
      return
    }
    if (token === 'offline') {
      response.writeHead(502, { 'content-type': 'text/plain' })
      response.end('captcha provider offline')
      return
    }
    if (token === 'reject') {
      json(response, 200, { success: false, 'error-codes': ['invalid-input-response'] })
      return
    }
    json(response, 200, { success: true, challenge_ts: new Date().toISOString() })
    return
  }
  if (request.method === 'GET' && url.pathname === '/healthz') {
    json(response, 200, { status: 'ok' })
    return
  }
  json(response, 404, { message: 'not found' })
})

server.listen(port, '0.0.0.0', () => {
  process.stdout.write(`captcha stub listening on ${port}\n`)
})
