# Yerel yamalı bağımlılıklar

Her klasör, crates.io'daki bir sürümün birebir kopyası + küçük bir P2 yamasıdır.
`Cargo.toml` içindeki `[patch.crates-io]` bölümüyle devreye girer. Yama üst projeye
önerilip kabul edildiğinde klasör ve patch satırı silinir.

| Klasör | Kaynak | Yama | Neden |
|---|---|---|---|
| `gpui-pre-windows` | crates.io `gpui-pre-windows 0.3.12` (Apache-2.0) | `src/platform.rs`, vsync döngüsü: gizli/küçültülmüş pencereye `RedrawWindow` gönderilmez | Yamasız sürüm, pencere simge durumundayken bile ana iş parçacığını her ekran tazelemesinde uyandırıyor (Faz 0 ölçümü: tek çekirdeğin ~%2,5'i) |
| `gpui-pre-windows` | (aynı) | `src/direct_write.rs` (`P2 yaması`): sahte `P2ls` özelliği → `SetCharacterSpacing`; değişken yazı tipinde `wght` = istenen tam ağırlık, `opsz` = yazı boyu (`SetFontAxisValues`, örneklenmiş yüz ayrı FontId); ClearType kapalı (gri tonlama, `P2_CLEARTYPE=1` açar) | Electron/Chromium ile birebir yazı: `letter-spacing`, 650 gibi ara ağırlıklar, `font-optical-sizing: auto`, gri tonlamalı yumuşatma (Faz 9 görsel parite) |

| `gpui-pre-wgpu` | crates.io `gpui-pre-wgpu 0.3.12` (Apache-2.0) | `src/wgpu_renderer.rs`, `WgpuHeadlessRenderer::render`: saydam temizleme + önceden çarpılmış alfa | Yamasız sürüm ekran dışı kareyi opak siyahla temizliyor; oyun içi overlay'de kartın dışındaki alan oyunu siyahla örtüyordu (Faz 0 / Spike 3) |

Sürüm yükseltirken: yeni sürümü kopyala, yamayı yeniden uygula, `P2 yaması` yorumunu ara.
| `gpui-pre` | crates.io `gpui-pre 0.3.12` (Apache-2.0) | `P2 yaması` yorumları: `style.rs` (`P2Transform`, `P2Backdrop`, `P2Filter` stil alanları; `Style::paint` backdrop işaretçisi), `scene.rs` (çizim zamanı dönüşüm/filtre yığını, `insert_primitive`'de uygulanır; `P2_BACKDROP_TAG`), `window.rs` (`with_p2_transform`, `with_p2_filter`, `paint_p2_backdrop`, `p2_font_scale`/`set_p2_font_scale` → `text_style()` px boyları), `elements/div.rs` (hover/grup-hover/active geçişleri `set_p2_default_transition`, `Interactivity::p2_transition`; Chromium kaydırma çubuğu `set_p2_scrollbars`, `Interactivity::p2_scrollbar`), `elements/list.rs` (kaydırma çubuğu) | CSS `transform`, `filter`, `backdrop-filter`, `transition`, `::-webkit-scrollbar`, `--app-font-scale` (Faz 9 görsel parite, ajan K) |
| `gpui-pre-windows` | (yukarıdaki) | `src/directx_renderer.rs` + `shaders.hlsl` + `build.rs` (`p2_blur`, `p2_backdrop` gölgelendiricileri): gölge grubunda `P2_BACKDROP_TAG` işaretçisinde hedefin öğe kutusu kopyalanır, iki geçişli Gauss (σ = CSS blur, ayna kenar), `saturate()` matrisiyle yuvarlak köşeli bileştirme | `backdrop-filter: blur()` (modal katmanları, menüler, cam paneller) |
