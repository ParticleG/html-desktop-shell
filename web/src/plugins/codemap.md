# `web/src/plugins/`

## Responsibility

`primevue.ts` centralizes application-wide PrimeVue installation and local theme policy so bootstrap and mounted tests reuse the same configuration.

## Export and configuration

- `installPrimeVue(app: App): void` is the only export; there is no barrel or default export.
- It registers `primevue/config` with the bundled Aura preset from `@primeuix/themes/aura`.
- `darkModeSelector: "system"` follows the embedded browser's color scheme.
- `cssLayer: false` leaves components in the normal cascade, allowing `styles.css` selectors for workspace buttons, surface actions, and status tags to override them by specificity/source order.

## Flow and integration

1. `main.ts` creates the Vue application, installs Pinia and i18n, calls `installPrimeVue(app)`, and mounts it.
2. Bar, Quick Panel, and Diagnostics views import PrimeVue buttons; workspace, battery, and network widgets import buttons/tags.
3. Mounted tests that render those controls install either this production installer or PrimeVue directly as appropriate.
4. Theme and component code are bundled local dependencies; the plugin uses no CDN, remote stylesheet, font, URL, or runtime-fetched asset.
