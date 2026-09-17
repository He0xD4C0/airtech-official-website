#!/usr/bin/env node

import { createHash } from 'node:crypto'
import { existsSync, lstatSync, readFileSync, readdirSync, realpathSync } from 'node:fs'
import { dirname, join, relative, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const IGNORED_DIRECTORIES = new Set([
  '.git',
  '.pnpm-store',
  'coverage',
  'dist',
  'node_modules',
  'playwright-report',
  'target',
  'test-results',
  'tmp',
])
const NUMBERED_COPY_PATTERN = /^(?<stem>.+) (?<copy>[2-9]\d*)\.(?<extension>[^.]+)$/u
const EXPECTED_PACKAGE_NAME = 'airtekpower-platform'

function digest(path) {
  return createHash('sha256').update(readFileSync(path)).digest('hex')
}

function visitDirectory(root, directory, findings) {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) {
      if (!IGNORED_DIRECTORIES.has(entry.name)) {
        visitDirectory(root, join(directory, entry.name), findings)
      }
      continue
    }
    if (!entry.isFile()) continue

    const match = NUMBERED_COPY_PATTERN.exec(entry.name)
    if (!match?.groups) continue
    const canonicalName = `${match.groups.stem}.${match.groups.extension}`
    const canonicalPath = join(directory, canonicalName)
    if (!existsSync(canonicalPath) || !lstatSync(canonicalPath).isFile()) continue

    const duplicatePath = join(directory, entry.name)
    findings.push({
      canonical: relative(root, canonicalPath),
      duplicate: relative(root, duplicatePath),
      identical: digest(canonicalPath) === digest(duplicatePath),
    })
  }
}

export function findNumberedCopies(root) {
  const findings = []
  visitDirectory(root, root, findings)
  return findings.sort((left, right) => left.duplicate.localeCompare(right.duplicate))
}

function visitEmptyDirectories(root, directory, findings) {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    if (!entry.isDirectory() || IGNORED_DIRECTORIES.has(entry.name)) continue
    const child = join(directory, entry.name)
    const childEntries = readdirSync(child, { withFileTypes: true })
    if (childEntries.length === 0) {
      findings.push(relative(root, child))
      continue
    }
    visitEmptyDirectories(root, child, findings)
  }
}

export function findEmptyDirectories(root) {
  const findings = []
  visitEmptyDirectories(root, root, findings)
  return findings.sort((left, right) => left.localeCompare(right))
}

function validateRoot(root, cwd) {
  if (resolve(cwd) !== resolve(root)) {
    throw new Error(`Run this command from the repository root: ${resolve(root)}`)
  }
  const packagePath = join(root, 'package.json')
  if (!existsSync(packagePath)) throw new Error('package.json is missing.')
  const packageJson = JSON.parse(readFileSync(packagePath, 'utf8'))
  if (packageJson.name !== EXPECTED_PACKAGE_NAME) {
    throw new Error(`Unexpected package name ${JSON.stringify(packageJson.name)}.`)
  }
}

export function checkWorkspaceHygiene({ root, cwd = process.cwd(), log = console.log } = {}) {
  validateRoot(root, cwd)
  const numberedCopies = findNumberedCopies(root)
  const emptyDirectories = findEmptyDirectories(root)
  if (numberedCopies.length === 0 && emptyDirectories.length === 0) {
    log('Workspace hygiene check passed: no numbered copies or empty directories found.')
    return []
  }

  for (const finding of numberedCopies) {
    const state = finding.identical ? 'identical copy' : 'content differs'
    log(`${finding.duplicate} conflicts with ${finding.canonical} (${state})`)
  }
  for (const directory of emptyDirectories) log(`${directory} is an empty directory`)
  throw new Error(
    'Workspace hygiene check failed: '
      + `${numberedCopies.length} numbered copies and ${emptyDirectories.length} empty directories found.`,
  )
}

const modulePath = fileURLToPath(import.meta.url)
if (process.argv[1] && realpathSync(process.argv[1]) === realpathSync(modulePath)) {
  try {
    checkWorkspaceHygiene({ root: resolve(dirname(modulePath), '..') })
  } catch (error) {
    console.error(error instanceof Error ? error.message : error)
    process.exitCode = 1
  }
}
