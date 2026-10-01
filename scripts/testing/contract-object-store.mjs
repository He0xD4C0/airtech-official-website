// Tests own this tmpfs-backed container and its random loopback port. Never use
// the development bucket or the fixed subnet of an existing Compose project.
export function contractObjectStore(name, run, capture) {
  if (!/^airtek(?:power)?-e2e(?:-[a-z0-9-]+)?-[a-f0-9]{12}-postgres-objects$/.test(name)) {
    throw new Error('Object-storage tests require an isolated test container name.')
  }
  let owned = false
  const minio = 'quay.io/minio/minio:RELEASE.2025-09-07T16-13-09Z@sha256:14cea493d9a34af32f524e538b8346cf79f3321eff8e708c1e2960462bd8936e'
  const mc = 'docker.io/minio/mc:RELEASE.2025-08-13T08-35-41Z@sha256:a7fe349ef4bd8521fb8497f55c6042871b2ae640607cf99d9bede5e9bdf11727'
  async function checked(args) {
    if (await run('docker', args) !== 0) throw new Error('Disposable object-storage setup failed.')
  }
  return {
    async start() {
      const existing = await capture('docker', ['container', 'ls', '--all', '--quiet', '--filter', `name=^/${name}$`])
      if (existing.status !== 0 || existing.stdout.trim()) throw new Error('Object-storage container name is already in use.')
      owned = true
      await checked(['run', '--detach', '--name', name, '--publish', '127.0.0.1::9000', '--tmpfs', '/data:rw',
        '-e', 'MINIO_ROOT_USER=airtek', '-e', 'MINIO_ROOT_PASSWORD=local-contract-only', minio, 'server', '/data'])
      let healthy = false
      for (let attempt = 0; attempt < 60; attempt++) {
        if ((await capture('docker', ['exec', name, 'curl', '--fail', '--silent', 'http://127.0.0.1:9000/minio/health/live'])).status === 0) {
          healthy = true
          break
        }
        await new Promise(resolve => setTimeout(resolve, 500))
      }
      if (!healthy) throw new Error('Disposable object storage did not start.')
      await checked(['run', '--rm', '--network', `container:${name}`, '--entrypoint', '/bin/sh',
        '--mount', `type=bind,source=${process.cwd()}/infra/object-storage/media-api-policy.json,target=/policy.json,readonly`,
        mc, '-ec', [
          'mc alias set local http://127.0.0.1:9000 airtek local-contract-only',
          'mc mb local/airtek-media',
          'mc anonymous set download local/airtek-media',
          'while IFS= read -r line; do',
          '  case "$line" in',
          '    *REPLACE_MEDIA_BUCKET*) printf "%sairtek-media%s\\n" "${line%%REPLACE_MEDIA_BUCKET*}" "${line#*REPLACE_MEDIA_BUCKET}" ;;',
          '    *) printf "%s\\n" "$line" ;;',
          '  esac',
          'done < /policy.json > /tmp/policy.json',
          'mc admin policy create local airtek-media-api /tmp/policy.json',
          'mc admin user add local airtek-media-api local-api-media-only',
          'mc admin policy attach local airtek-media-api --user airtek-media-api',
        ].join('\n')])
      const port = (await capture('docker', ['port', name, '9000/tcp'])).stdout.trim().split(':').at(-1)
      if (!/^\d+$/.test(port)) throw new Error('No isolated object-storage port.')
      return `http://127.0.0.1:${port}`
    },
    async cleanup() {
      return !owned || await run('docker', ['rm', '--force', name], { cleanup: true }) === 0
    },
  }
}
