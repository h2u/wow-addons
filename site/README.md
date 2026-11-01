# Landing page

Static GitHub Pages site for WoW Addons. No build step or dependencies.

The page lives in `site/`. The screenshot and app icon are copied from
`docs/images/wow-addons.png` and `desktop/assets/app-icon.png`; update the copies
in `site/assets/` when those assets change.

## Publish

1. Merge `site/` and `.github/workflows/pages.yml` into `main`.
2. In the repository's **Settings → Pages → Build and deployment**, select
   **GitHub Actions** as the source (the existing configuration uses a branch).
3. Run **Actions → Deploy landing page → Run workflow** on `main` if the merge
   workflow did not deploy because the source setting was changed afterward.

Public URL: https://h2u.github.io/wow-addons/

For local preview, serve `site/` with any static HTTP server.
