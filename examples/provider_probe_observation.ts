// Bounded diagnostics for an already authenticated gateway's SSE output.
// Not a replacement for the protocol decoder: only answer text and terminal
// facts are observed. Reasoning, opaque values, identities and headers are never
// retained in reports. It does not rewrite bytes or relax a scenario oracle.
import { record, array } from './probe_values.ts';
import type { Protocol } from './probe_values.ts';

export function classifyText(text: unknown, expected: string) {
  if (typeof text !== 'string') return 'unavailable';
  const value = text.trim();
  if (value === expected) return 'exact';
  if (!value) return 'empty';
  if (value.toLowerCase() === expected.toLowerCase()) return 'case_variant';
  try { if (JSON.parse(value) === expected) return 'quoted'; } catch { /* Not JSON. */ }
  if (['.', '!', '。', '！'].some(mark => value === expected + mark)) return 'punctuated';
  if (/^```[\s\S]*```$/.test(value) && value.includes(expected)) return 'fenced';
  return value.includes(expected) ? 'extra_text' : 'other';
}

export class WireObservation {
  #protocol: Protocol; #limits: {wireBytes: number; frameBytes: number; textChars: number}; #decoder = new TextDecoder('utf-8', {fatal:true});
  #pending = ''; #bytes = 0; #parts = new Map<string, string>(); #textChars = 0;
  #snapshot: string | undefined; #terminal: string | undefined; #done = false; #eof = false; #failed = false;
  constructor(protocol: Protocol, {wireBytes = 2 << 20, frameBytes = 1 << 20, textChars = 65536} = {}) {
    if (!['chat','responses'].includes(protocol) || [wireBytes,frameBytes,textChars].some(n => !Number.isSafeInteger(n) || n < 1)) {
      throw new Error('invalid observation limits or protocol');
    }
    this.#protocol = protocol;
    this.#limits = {wireBytes,frameBytes,textChars};
  }
  push(bytes: Uint8Array) {
    if (this.#failed || this.#eof) throw new Error('closed observation');
    try {
      this.#bytes += bytes.byteLength;
      if (this.#bytes > this.#limits.wireBytes) throw new Error('observation wire budget');
      this.#pending += this.#decoder.decode(bytes, {stream:true});
      let delimiter;
      while ((delimiter = /\r?\n\r?\n/.exec(this.#pending)) !== null) {
        const frame = this.#pending.slice(0,delimiter.index);
        this.#pending = this.#pending.slice(delimiter.index + delimiter[0].length);
        if (Buffer.byteLength(frame) > this.#limits.frameBytes) throw new Error('observation frame budget');
        this.#frame(frame);
      }
      // Allow only the bounded, not-yet-complete CRLF delimiter suffix. The
      // complete frame is charged before any newline normalization or parsing.
      if (Buffer.byteLength(this.#pending) > this.#limits.frameBytes + 3) throw new Error('observation frame budget');
    } catch {
      this.#failed = true;
      throw new Error('invalid or over-budget diagnostic stream');
    }
  }
  #append(key: string, text: unknown) {
    if (typeof text !== 'string') throw new Error('invalid text delta');
    this.#textChars += text.length;
    if (this.#textChars > this.#limits.textChars) throw new Error('observation text budget');
    this.#parts.set(key,(this.#parts.get(key) ?? '') + text);
  }
  #frame(frame: string) {
    const data = frame.split(/\r?\n/).filter(line => line.startsWith('data:')).map(line => line.slice(5).replace(/^ /,'')).join('\n');
    if (!data) return;
    if (this.#done) throw new Error('data after terminal');
    if (data === '[DONE]') {
      if (this.#protocol !== 'chat' || !this.#terminal) throw new Error('premature DONE');
      this.#done = true;
      return;
    }
    const value = record(JSON.parse(data));
    if (this.#protocol === 'responses') {
      if (value.type === 'response.output_text.delta') {
        if (![value.output_index,value.content_index].every(n => typeof n === 'number' && Number.isSafeInteger(n) && n >= 0)) throw new Error('invalid text coordinates');
        this.#append(`${value.output_index}:${value.content_index}`,value.delta);
      } else if (typeof value.type === 'string' && ['response.completed','response.incomplete','response.failed','error'].includes(value.type)) {
        this.#terminal = String(value.type);
        if (value.type === 'response.completed') {
          const parts = array(record(value.response).output).map(record).filter(item => item.type === 'message')
            .flatMap(item => array(item.content ?? []).map(record)).filter(part => part.type === 'output_text');
          const texts = parts.map(part => {
            if (typeof part.text !== 'string') throw new Error('invalid terminal text');
            return part.text;
          });
          if (texts.reduce((n,text) => n + text.length,0) > this.#limits.textChars) throw new Error('terminal text budget');
          this.#snapshot = texts.join('');
        }
        this.#done = true;
      }
    } else {
      for (const choice of array(value.choices ?? []).map(record)) {
        if (choice.index !== 0) throw new Error('multiple candidates');
        const delta = record(choice.delta ?? {});
        if (delta.content != null) this.#append('0:0',delta.content);
        if (choice.finish_reason != null) {
          if (typeof choice.finish_reason !== 'string' || !['stop','tool_calls','length','content_filter'].includes(choice.finish_reason)) throw new Error('unknown finish');
          this.#terminal = choice.finish_reason;
        }
      }
    }
  }
  finish() {
    if (this.#failed || this.#eof) throw new Error('closed observation');
    try {
      this.#pending += this.#decoder.decode();
      if (this.#pending.trim()) throw new Error('truncated frame');
      this.#eof = true;
    } catch {
      this.#failed = true;
      throw new Error('truncated diagnostic stream');
    }
  }
  #deltaText() {
    return [...this.#parts].sort(([a],[b]) => {
      const [ai,ac] = a.split(':').map(Number), [bi,bc] = b.split(':').map(Number);
      return ai - bi || ac - bc;
    }).map(([,text]) => text).join('');
  }
  facts(expected: string, consumerText?: string) {
    const available = !this.#failed && this.#done && (this.#protocol === 'chat' || this.#terminal === 'response.completed');
    const text = available ? (this.#protocol === 'responses' ? this.#snapshot : this.#deltaText()) : undefined;
    return {
      eof:this.#eof, terminal:this.#terminal, wire_available:available,
      wire_chars:text?.length, wire_class:classifyText(text,expected),
      delta_snapshot_equal:available && this.#protocol === 'responses' ? this.#deltaText() === text : undefined,
      consumer_matches_wire:available && typeof consumerText === 'string' ? consumerText === text : undefined,
    };
  }
}
