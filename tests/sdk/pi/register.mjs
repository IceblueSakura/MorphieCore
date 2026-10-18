// Resolve only the pinned pi source's external imports through isolated probe dependencies.
import { registerHooks } from 'node:module';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
const source = pathToFileURL(resolve(process.argv[2])).href + '/';
const root = new URL('./package.json', import.meta.url).href;
registerHooks({ resolve(specifier, context, nextResolve) {
  if (!specifier.startsWith('.') && !specifier.startsWith('/') && !specifier.includes(':')
      && context.parentURL?.startsWith(source)) {
    return nextResolve(specifier, { ...context, parentURL: root });
  }
  return nextResolve(specifier, context);
}});
