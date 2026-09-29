/*!
 * The page the browser lands on after a provider sign-in (Neon, Supabase,
 * PlanetScale, Prisma, TiDB, Turso, Cloudflare).
 *
 * It is served by Stroke's own localhost callback listener, so it can't load
 * anything: no network assets, no fonts from a CDN. The logo is inlined as a data
 * URI and the page follows the browser's light/dark preference. It says who is
 * talking (Stroke, with a link to the site), what happened, and what to do next,
 * which is usually nothing but switching back to the app.
 */

use base64::{engine::general_purpose::STANDARD, Engine};
use std::sync::OnceLock;

const WEBSITE: &str = "https://stroke.click";

/// White mark for dark pages, dark mark for light ones (see Logo.svelte).
fn logos() -> &'static (String, String) {
    static LOGOS: OnceLock<(String, String)> = OnceLock::new();
    LOGOS.get_or_init(|| {
        let uri = |png: &[u8]| format!("data:image/png;base64,{}", STANDARD.encode(png));
        (
            uri(include_bytes!("../../public/stroke_white.png")),
            uri(include_bytes!("../../public/stroke_light.png")),
        )
    })
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// `utm_source` says the visit came from the app, `utm_medium` which surface
/// sent it, `utm_campaign` which provider's sign-in, `utm_content` how it went.
fn website_link(provider: &str, ok: bool) -> String {
    let slug: String = provider
        .chars()
        .filter_map(|c| if c.is_ascii_alphanumeric() { Some(c.to_ascii_lowercase()) } else if c == ' ' { Some('-') } else { None })
        .collect();
    format!(
        "{WEBSITE}/?utm_source=stroke-app&amp;utm_medium=oauth-callback&amp;utm_campaign={slug}&amp;utm_content={}",
        if ok { "success" } else { "error" }
    )
}

/// The full HTML for the callback tab. `provider` is the display name
/// ("Neon", "Cloudflare"); `ok` picks the success or failure copy.
pub fn page(ok: bool, provider: &str) -> String {
    let (logo_dark_ui, logo_light_ui) = logos();
    let link = website_link(provider, ok);
    let provider = escape(provider);
    let (title, status, heading, body, tone, icon) = if ok {
        (
            format!("Signed in to {provider} · Stroke"),
            "Signed in",
            format!("You're connected to {provider}"),
            "Switch back to Stroke to pick a database. This tab can be closed.",
            "ok",
            r#"<path d="M20 6 9 17l-5-5"/>"#,
        )
    } else {
        (
            format!("{provider} sign-in failed · Stroke"),
            "Not signed in",
            format!("{provider} sign-in didn't finish"),
            "Nothing was saved. Close this tab and start the sign-in again from Stroke.",
            "err",
            r#"<path d="M18 6 6 18"/><path d="m6 6 12 12"/>"#,
        )
    };

    // One card, one leading edge: brand row, a status pill, the heading and the
    // next step, all left-aligned. The first version centred a lone logo above
    // a lone check circle, two marks stacked with nothing tying them together.
    format!(
        r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="color-scheme" content="dark light">
<title>{title}</title>
<style>
  :root {{
    --bg: #0b0b0c; --surface: #141416; --border: #27272b;
    --fg: #ededee; --muted: #a1a1a8;
    --ok: #3ecf8e; --ok-bg: rgb(62 207 142 / .12);
    --err: #f87171; --err-bg: rgb(248 113 113 / .12);
    --ring: #7c93ff;
  }}
  @media (prefers-color-scheme: light) {{
    :root {{
      --bg: #f6f6f7; --surface: #ffffff; --border: #e4e4e7;
      --fg: #18181b; --muted: #5b5b63;
      --ok: #15803d; --ok-bg: rgb(21 128 61 / .10);
      --err: #b91c1c; --err-bg: rgb(185 28 28 / .10);
      --ring: #3b5bdb;
    }}
  }}
  * {{ box-sizing: border-box; }}
  html, body {{ height: 100%; }}
  body {{
    margin: 0; background: var(--bg); color: var(--fg);
    font: 15px/1.5 ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif;
    -webkit-font-smoothing: antialiased;
    display: flex; flex-direction: column; align-items: center; justify-content: center;
    gap: 16px; padding: 24px 16px;
  }}
  main {{
    width: 100%; max-width: 420px;
    background: var(--surface); border: 1px solid var(--border); border-radius: 16px;
    box-shadow: 0 1px 2px rgb(0 0 0 / .05), 0 12px 32px rgb(0 0 0 / .08);
    overflow: hidden;
  }}
  .brand {{
    display: flex; align-items: center; gap: 10px;
    padding: 14px 24px; border-bottom: 1px solid var(--border);
    font-size: 14px; font-weight: 600; letter-spacing: -0.01em;
  }}
  .brand img {{ width: 20px; height: 20px; object-fit: contain; }}
  .brand .light {{ display: none; }}
  @media (prefers-color-scheme: light) {{
    .brand .dark {{ display: none; }}
    .brand .light {{ display: block; }}
  }}
  .content {{ padding: 24px; }}
  .pill {{
    display: inline-flex; align-items: center; gap: 6px;
    padding: 3px 10px 3px 8px; border-radius: 999px;
    font-size: 12px; font-weight: 500; line-height: 18px;
  }}
  .pill svg {{ width: 14px; height: 14px; fill: none; stroke: currentColor;
    stroke-width: 2.5; stroke-linecap: round; stroke-linejoin: round; }}
  .ok {{ color: var(--ok); background: var(--ok-bg); }}
  .err {{ color: var(--err); background: var(--err-bg); }}
  h1 {{ margin: 14px 0 6px; font-size: 19px; line-height: 1.3; font-weight: 600;
    letter-spacing: -0.015em; text-wrap: balance; }}
  p {{ margin: 0; color: var(--muted); text-wrap: pretty; }}
  footer {{ width: 100%; max-width: 420px; display: flex; justify-content: space-between;
    gap: 12px; padding: 0 4px; font-size: 13px; color: var(--muted); }}
  a {{ color: var(--fg); text-decoration: underline; text-underline-offset: 3px;
    text-decoration-color: color-mix(in srgb, currentColor 35%, transparent); }}
  a:hover {{ text-decoration-color: currentColor; }}
  a:focus-visible {{ outline: 2px solid var(--ring); outline-offset: 2px; border-radius: 4px; }}
  @media (prefers-reduced-motion: no-preference) {{
    main {{ animation: rise .32s cubic-bezier(0.2, 0, 0, 1) both; }}
    @keyframes rise {{ from {{ opacity: 0; transform: translateY(6px); }} to {{ opacity: 1; transform: none; }} }}
  }}
</style>
</head>
<body>
<main>
  <div class="brand">
    <img class="dark" src="{logo_dark_ui}" alt="">
    <img class="light" src="{logo_light_ui}" alt="">
    <span>Stroke</span>
  </div>
  <div class="content">
    <span class="pill {tone}"><svg viewBox="0 0 24 24" aria-hidden="true">{icon}</svg>{status}</span>
    <h1>{heading}</h1>
    <p>{body}</p>
  </div>
</main>
<footer>
  <span>The database studio for agents and humans</span>
  <a href="{link}" target="_blank" rel="noopener">stroke.click</a>
</footer>
</body>
</html>"##
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_carries_utm_for_the_provider_and_outcome() {
        let html = page(true, "TiDB Cloud");
        assert!(html.contains("utm_source=stroke-app&amp;utm_medium=oauth-callback&amp;utm_campaign=tidb-cloud&amp;utm_content=success"));
        assert!(page(false, "Neon").contains("utm_campaign=neon&amp;utm_content=error"));
    }

    #[test]
    fn provider_name_is_escaped() {
        assert!(page(true, "<x>").contains("&lt;x&gt;"));
    }
}
