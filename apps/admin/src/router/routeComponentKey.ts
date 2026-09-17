import type { RouteLocationNormalizedLoaded } from 'vue-router'

type RouteIdentity = Pick<RouteLocationNormalizedLoaded, 'name' | 'path'>

export function routeComponentKey(route: RouteIdentity): string {
  return String(route.name ?? route.path)
}
