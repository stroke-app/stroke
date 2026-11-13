const URL_RE = /^https?:\/\/\S+$/i
// File URLs whose name has spaces in it (`…/photo 1 (5).jpg`), as uploads often
// do. Spaces are only accepted when the text ends in a file extension, so a
// sentence that starts with a link is still plain text. Encoded before use.
const SPACED_FILE_URL_RE = /^https?:\/\/\S+(?: +\S+)*\.(jpe?g|png|gif|webp|svg|avif|bmp|ico|pdf)(\?\S*)?$/i
const IMAGE_EXT_RE = /\.(jpe?g|png|gif|webp|svg|avif|bmp|ico)(\?.*)?$/i
const PDF_EXT_RE = /\.pdf(\?.*)?$/i
// Many image URLs carry no file extension (CDNs, signed/S3 URLs, image hosts).
// When the column name signals an image, treat any http(s) link as one.
const IMAGE_COL_RE =
  /(image|img|photo|avatar|thumb(?:nail)?|picture|banner|logo|cover|icon|media|attachment|gallery|profile_?pic|headshot)/i
// Well-known image hosts/CDNs that serve images without a path extension.
const IMAGE_HOST_RE =
  /(images?\.|img\.|cdn\.|\.cloudfront\.net|\.imgix\.net|res\.cloudinary\.com|\.googleusercontent\.com|gravatar\.com|unsplash\.com|imgur\.com|format=(?:jpe?g|png|webp|avif))/i

/**
 * @param {string} text
 * @returns {string | null} href for http(s) cells only (emails stay plain text)
 */
export function cellLinkHref(text) {
  const t = text.trim()
  if (!t) return null
  if (!URL_RE.test(t) && !SPACED_FILE_URL_RE.test(t)) return null
  // Percent-encode before the value is ever fetched (image preview) or opened.
  // `new URL().href` normalizes unsafe characters - spaces, unicode, brackets -
  // and is idempotent, so an already-encoded URL is left untouched (no
  // %20 -> %2520 double-encoding). Falls back to the raw match if unparseable.
  try {
    return new URL(t).href
  } catch {
    try { return encodeURI(t) } catch { return t }
  }
}

/**
 * @param {string | null} href
 * @param {string} [colName] column name, used as a hint for extension-less image URLs
 * @returns {'image' | 'pdf' | 'link' | null}
 */
export function cellUrlType(href, colName = '') {
  if (!href) return null
  let path = href
  try { path = new URL(href).pathname } catch { /* use raw href */ }
  if (IMAGE_EXT_RE.test(path)) return 'image'
  if (PDF_EXT_RE.test(path)) return 'pdf'
  // Extension-less URLs only: trust the column name or a known image host/CDN.
  // Skipping URLs that already carry a (non-image) extension avoids treating
  // e.g. a `.css`/`.js` asset on a `cdn.` host as an image.
  const hasExt = /\.[a-z0-9]{1,5}(\?.*)?$/i.test(path)
  if (!hasExt && (IMAGE_COL_RE.test(colName) || IMAGE_HOST_RE.test(href))) return 'image'
  return 'link'
}
