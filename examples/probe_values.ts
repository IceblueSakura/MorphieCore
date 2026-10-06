// Runtime validation for untrusted JSON; TypeScript annotations are not admission.
export type Protocol = 'chat' | 'responses';
export function record(value: unknown): Record<string, unknown> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) throw new Error('Expected object');
  return value as Record<string, unknown>;
}
export function array(value: unknown): unknown[] {
  if (!Array.isArray(value)) throw new Error('Expected array');
  return value;
}
export function protocol(value: unknown): Protocol {
  if (value !== 'chat' && value !== 'responses') throw new Error('Expected explicit protocol');
  return value;
}
