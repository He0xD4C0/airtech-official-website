# Content renderer package

`@airtek/content-renderer` owns the Admin draft visual canvas. It turns an
editable draft document into a safe in-memory preview and may consume pending
local media URLs supplied by the Admin application.

This package is not the Public website renderer and does not define published
page behavior. Public SSR continues to use the Web application's typed public
projection and block renderer. Keep this package name and export stable until a
separate, reviewed change either introduces a second consumer or returns the
canvas to Admin ownership.
