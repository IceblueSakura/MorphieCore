// Pure header projection for a local test relay, not a credential provider.
// Preserve the caller's actual values; only the real gateway decides admission.
// In particular, never substitute the harness's known-good client token.
export function relayHeaders(rawHeaders: readonly string[]): Record<string, string> {
  if (!Array.isArray(rawHeaders) || rawHeaders.length % 2) throw new Error('invalid relay headers');
  const headers: Record<string, string> = {};
  for (let i = 0; i < rawHeaders.length; i += 2) {
    if (typeof rawHeaders[i] !== 'string' || typeof rawHeaders[i + 1] !== 'string') throw new Error('invalid relay header');
    const name = rawHeaders[i].toLowerCase();
    if (!['authorization','content-type'].includes(name)) continue;
    if (Object.hasOwn(headers,name)) throw new Error('duplicate relay header');
    headers[name] = rawHeaders[i + 1];
  }
  return headers;
}
