import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'

const require=createRequire(import.meta.url)
const libarchiveWasm=require.resolve('libarchive.js/dist/libarchive.wasm')

export default defineConfig({
  plugins: [react(),{
    // libarchive worker üretimde WASM dosyasını kendi klasöründe bu sabit adla arar.
    name:'p2-libarchive-wasm',
    generateBundle(){this.emitFile({type:'asset',fileName:'assets/libarchive.wasm',source:readFileSync(libarchiveWasm)})},
  }],
  base: './',
  server: {
    // 0.0.0.0: aynı ağdaki telefon/tabletten de açılabilsin
    host: true,
    // Tünel adresleri (ör. *.trycloudflare.com) da kabul edilsin; geliştirme sunucusu
    // varsayılan olarak yalnızca bilinen host başlıklarına yanıt veriyor.
    allowedHosts: true,
    port: 5173,
    strictPort: true,
    // Yerel overlay derlemesi (cmake/node-gyp) dev sunucusu açıkken de yapılabilsin; izleyici
    // bu klasörlere bakınca EBUSY ile çöküyordu (dosyalar derleyici tarafından kilitli).
    watch: { ignored: ['**/native/**', '**/server/**', '**/release-package/**', '**/dist/**', '**/dist-server/**', '**/electron/**'] },
    // API ve websocket aynı adres altından sunulur: uzaktan erişimde tek bir
    // HTTPS adresi yeterli olur, karışık içerik (https sayfa -> http API) sorunu çıkmaz.
    proxy: {
      '/api': { target: `http://127.0.0.1:${process.env.P2_DEV_API_PORT||4000}`, changeOrigin: true, rewrite: path => path.replace(/^\/api/, '') },
      '/socket.io': { target: `http://127.0.0.1:${process.env.P2_DEV_API_PORT||4000}`, ws: true, changeOrigin: true },
    },
  },
})
