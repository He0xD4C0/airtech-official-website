import { existsSync, lstatSync, readdirSync } from 'node:fs'
import { join, relative, resolve } from 'node:path'
import { pathToFileURL } from 'node:url'

export function findDependencyCopies(root) {
  const findings = []
  const workspaces = ['.', ...['apps', 'packages'].flatMap(parent =>
    existsSync(join(root, parent))
      ? readdirSync(join(root, parent), { withFileTypes: true })
        .filter(entry => entry.isDirectory()).map(entry => `${parent}/${entry.name}`)
      : [])]
  function inspect(directory) {
    if (!existsSync(directory)) return
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      const match = /^(.*) [2-9]\d*$/u.exec(entry.name)
      if (match) {
        try {
          lstatSync(join(directory, match[1]))
          findings.push(relative(root, join(directory, entry.name)))
        } catch (error) {
          if (error.code !== 'ENOENT') throw error
        }
      }
      if (entry.isDirectory() && entry.name.startsWith('@')) inspect(join(directory, entry.name))
    }
  }
  for (const workspace of workspaces) inspect(join(root, workspace, 'node_modules'))
  return findings.sort()
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const findings = findDependencyCopies(process.cwd())
  if (findings.length) {
    console.error(`Numbered dependency copies found:\n${findings.join('\n')}`)
    process.exitCode = 1
  } else console.log('Dependency hygiene check passed.')
}
