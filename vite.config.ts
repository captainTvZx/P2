import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

export default defineConfig({
  plugins: [react()],
  base: './',
  server: {
    // 0.0.0.0: aynı ağdaki telefon/tabletten de açılabilsin
    host: true,
    // Tünel adresleri (ör. *.trycloudflare.com) da kabul edilsin; geliştirme sunucusu
    // varsayılan olarak yalnızca bilinen host başlıklarına yanıt veriyor.
    allowedHosts: true,
    port: 5173,
    strictPort: true,
    // API ve websocket aynı adres altından sunulur: uzaktan erişimde tek bir
    // HTTPS adresi yeterli olur, karışık içerik (https sayfa -> http API) sorunu çıkmaz.
    proxy: {
      '/api': { target: 'http://127.0.0.1:4000', changeOrigin: true, rewrite: path => path.replace(/^\/api/, '') },
      '/socket.io': { target: 'http://127.0.0.1:4000', ws: true, changeOrigin: true },
    },
  },
})
