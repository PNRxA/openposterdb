import { readdirSync, existsSync } from 'node:fs'
import { join, extname } from 'node:path'
import type { Plugin } from 'vite'

const VIRTUAL_ID = 'virtual:placeholders'
const RESOLVED_ID = '\0' + VIRTUAL_ID
const IMAGE_EXTS = new Set(['.jpg', '.jpeg', '.png'])

export default function placeholdersPlugin(): Plugin {
  let publicDir: string

  return {
    name: 'placeholders',

    configResolved(config) {
      publicDir = config.publicDir
    },

    resolveId(id) {
      if (id === VIRTUAL_ID) return RESOLVED_ID
    },

    async load(id) {
      if (id !== RESOLVED_ID) return

      const examplesDir = join(publicDir, 'examples')
      if (!existsSync(examplesDir)) {
        return `export const placeholders = {};\n`
      }
      const files = readdirSync(examplesDir).filter(f =>
        IMAGE_EXTS.has(extname(f).toLowerCase())
      )

      const sharp = (await import('sharp')).default

      const entries: string[] = []
      for (const file of files) {
        const filePath = join(examplesDir, file)
        const ext = extname(file).toLowerCase()
        const mime = ext === '.png' ? 'image/png' : 'image/jpeg'
        const format = ext === '.png' ? 'png' : 'jpeg'

        const buf = await sharp(filePath)
          .resize({ width: 20 })
          .toFormat(format, format === 'jpeg' ? { quality: 20 } : {})
          .toBuffer()

        const b64 = `data:${mime};base64,${buf.toString('base64')}`
        entries.push(`  ${JSON.stringify('/examples/' + file)}: ${JSON.stringify(b64)}`)
      }

      return `export const placeholders = {\n${entries.join(',\n')}\n};\n`
    },

    configureServer(server) {
      const examplesDir = join(publicDir, 'examples')
      server.watcher.add(examplesDir)
      server.watcher.on('all', (event, path) => {
        if (path.startsWith(examplesDir) && IMAGE_EXTS.has(extname(path).toLowerCase())) {
          const mod = server.moduleGraph.getModuleById(RESOLVED_ID)
          if (mod) {
            server.moduleGraph.invalidateModule(mod)
            server.hot.send({ type: 'full-reload' })
          }
        }
      })
    },
  }
}
