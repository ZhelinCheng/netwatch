/*
 * @Author       : 程哲林
 * @Date         : 2026-06-20 18:18:32
 * @LastEditors  : 程哲林
 * @LastEditTime : 2026-06-20 18:25:20
 * @FilePath     : /netwatch/fe/vite.config.ts
 * @Description  : 未添加文件描述
 */
import { defineConfig } from 'vite'
import react, { reactCompilerPreset } from '@vitejs/plugin-react'
import babel from '@rolldown/plugin-babel'
import path from 'path'
const outPath = path.resolve(__dirname, '..', 'dashboard')

// https://vite.dev/config/
export default defineConfig({
  plugins: [react(), babel({ presets: [reactCompilerPreset()] })],
  server: {
    host: 'localhost',
    port: 5173,
    proxy: {
      '/api': {
        target: 'http://127.0.0.1:4311',
        changeOrigin: true,
      },
    },
  },
  build: {
    outDir: outPath,
    minify: true,
    emptyOutDir: true,
  },
})
