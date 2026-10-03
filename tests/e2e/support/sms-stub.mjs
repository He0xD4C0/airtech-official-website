// Acceptance-stack SMS provider stub.
//
// The API posts the Aliyun `SendSms` form to AIRTEK_SMS_ENDPOINT when the
// isolated stack enables the override. The stub answers like Aliyun and keeps
// the last code per phone so the browser journey can read it back over HTTP.
import { createServer } from 'node:http'

const port = Number(process.env.SMS_STUB_PORT ?? 8099)
const codes = new Map()

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
  if (request.method === 'POST' && url.pathname === '/sms') {
    const form = new URLSearchParams(await readBody(request))
    const phone = form.get('PhoneNumbers') ?? ''
    const template = form.get('TemplateParam') ?? ''
    const code = /"code"\s*:\s*"([0-9]{6})"/u.exec(template)?.[1]
    if (phone && code) codes.set(phone, { phone, code, receivedAt: new Date().toISOString() })
    json(response, 200, { Code: 'OK', Message: 'OK' })
    return
  }
  if (request.method === 'GET' && url.pathname === '/codes/latest') {
    const entry = codes.get(url.searchParams.get('phone') ?? '')
    if (!entry) {
      json(response, 404, { message: 'no code recorded for that phone number' })
      return
    }
    json(response, 200, entry)
    return
  }
  if (request.method === 'GET' && url.pathname === '/healthz') {
    json(response, 200, { status: 'ok', phones: codes.size })
    return
  }
  json(response, 404, { message: 'not found' })
})

server.listen(port, '0.0.0.0', () => {
  process.stdout.write(`sms stub listening on ${port}\n`)
})
