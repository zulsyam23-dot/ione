# ione — Rencana Fitur Baru (Update)

> Daftar fitur yang **belum diimplementasikan** beserta prioritasnya.
> Untuk fitur yang **sudah ada**, lihat **[`FITUR.md`](FITUR.md)** — dokumen ini hanya mencatat yang kurang.

---

## Daftar Isi

- [A. Prioritas Tinggi](#a-prioritas-tinggi)
- [B. Prioritas Sedang](#b-prioritas-sedang)
- [C. Performa & Risiko](#c-performa--risiko)
- [Catatan Verifikasi](#catatan-verifikasi)

---

## A. Prioritas Tinggi

| # | Fitur | Dampak | Bukti |
|---|-------|--------|-------|
| 1 | **Undo/redo untuk multi-cursor** — undo saat ini hanya kursor tunggal (bawaan egui 0.36); batch multi-select tidak bisa dibatalkan | Kesalahan edit menyebar ke banyak kursor tanpa jalan mundur | `multi.rs:6` ("no multi clipboard/undo") |
| 2 | **Clipboard multi-cursor** — copy/cut/paste hanya berfungsi di kursor tunggal (default egui TextEdit); mode Ctrl+D tak punya operasi klip & paste tidak bisa disebar ke semua kursor | Kolaboratif editing terhambat | `multi.rs:6` |
| 3 | **LSP / intelijen bahasa** — go-to-definition, hover docs, peek, rename simbol, error asli compiler | Deteksi error & navigasi masih manual; lint lokal tak tahu tipe | `diagnostics.rs:7` (sengaja disiapkan sebagai pintu masuk LSP) |
| 4 | **Format otomatis** (rustfmt/Prettier) + format-on-save | Konsistensi style harus dijaga manual | Tidak ada aksi Format di menu (`menu.rs`) |
| 5 | **Auto-close pasangan kurung/kutip + auto-indent** ✅ SELESAI — `(` → `()`, `"` → `""`, dst.; Enter membawa indentasi baris sebelumnya; tidak berlaku di dalam string/komentar | Kecepatan mengetik; indentasi blok `{ … }` manual | `src/editor/mod.rs` (pass auto-close, pre-frame event splice) |
| 6 | **Quick open** (Ctrl+P) ✅ SELESAI — daftar file workspace, filter live, Enter/klik buka (data dari file-tree; repo besar terbatas pada folder yang di-expand) | Membuka file cepat antar-workspace tidak ada; Ctrl+K chord hanya OpenFolder | `app/mod.rs` (`show_quick_open`), `menu.rs` Ctrl+P |
| 7 | **Integrasi build/run** — `cargo check`/`cargo run` → squiggle & output ke terminal | Umpan balik compiler masih manual | Terminal ada (`terminal.rs`) tapi tidak terhubung ke diagnostics |

## B. Prioritas Sedang

| # | Fitur | Dampak | Bukti |
|---|-------|--------|-------|
| 8 | **Search/replace regex** | Pencarian kompleks (anchor, group, alternation) mustahil | `search.rs` (hanya teks polos, case-sensitive) |
| 9 | **Replace across files** | Ganti-banyak hanya dalam file aktif | `actions.rs:284` (`ReplaceAll` → `replace_all_in_content`, satu buffer) |
| 10 | **Split editor** (side-by-side / horizontal) | Tidak bisa bandingkan dua file berdampingan | `app/mod.rs` (layout satu pane editor) |
| 11 | **Seleksi kolom/blok (Alt+drag) + select-all-occurrences (Ctrl+Shift+L) + add cursor per baris** | Multi-cursor hanya Ctrl+D "next" | `multi.rs` |
| 12 | **Operasi baris** ✅ SELESAI — Ctrl+Shift+D duplikat, Alt+↑/↓ pindah baris, Ctrl+Shift+K hapus baris, Ctrl+/ komentar | Refactor cepat terhambat | `src/editor/ops.rs` (buffer murni) + `editor/mod.rs` (post-frame) |
| 13 | **Snippets** (bawaan + user-defined + tab-stop) | Template kode (loop, fn, HTML) diketik ulang | Tidak ada |
| 14 | **Recent files & session restore** ✅ SEBAGIAN — recent files jalan (`%APPDATA%\ione\recent.txt`, menu File → Open Recent, cap 10); restore tab terakhir antar-sesi belum | Buka lagi file yang tadi dibuka harus manual | `src/settings.rs`; `menu.rs` |
| 15 | **Toggle word wrap** | Baris panjang tak terbungkus | Layout satu baris; gutter sengaja "never crossing a wrap" (`diagnostics.rs:94`) |
| 16 | **Block comment toggle (Ctrl+/)** ✅ SELESAI — toggle `// ` per baris (CSV/terminal: `#`) pada baris non-blank, sekali jalan untuk seleksi multi-baris | Komentar manual | `src/editor/ops.rs` |
| 17 | **UI pengaturan persisted** ✅ SEBAGIAN — tema + font tersimpan antar sesi (`settings.txt`); keybinding custom, tab width, font size masih hardcoded | Preferensi hilang tiap buka; `FONT_SIZE=14.0` hardcoded | `src/settings.rs`, `editor/mod.rs`; hanya font picker yang ada (`menu.rs:146`) |

## C. Performa & Risiko

| # | Fitur | Dampak | Bukti |
|---|-------|--------|-------|
| 18 | **Virtual rendering file besar** | LayoutJob dibangun utuh tiap frame → file besar melambat | `editor/mod.rs` |
| 19 | **Normalisasi encoding/BOM & newline + perbaiki mojibake literal** | Placeholder spasi & marker fold tampil sebagai karakter aneh (`â\x90£`, `â‹¯`) | Byte `C3 A2 C2 90 C2 A3` (`SPACE_HOLDER`, `editor/mod.rs`), `C3 A2 E2 80 9B C2 AF` (`MARKER`, `editor/folds.rs`) |
| 20 | **Minimap & breadcrumb** | Navigasi file besar & jejak posisi tak ada | — |
| 21 | **Panel Git** (status, stage, commit, diff, blame) | Version control harus keluar aplikasi | — |
| 22 | **Filter cepat di file-tree** | Mencari file dalam pohon besar lambat | `file_tree.rs` (tanpa kotak filter) |
| 23 | **Shell non-PowerShell** (cmd/bash/WSL) | Pengguna non-PowerShell tak bisa | Hardcoded pwsh/powershell (`terminal.rs:132`) |
| 24 | **Drag-drop file ke editor/terminal** | Buka file dengan drag belum didukung | — |
| 25 | **Optimasi bracket guide O(pairs×rows)** | Akses kursor per pasangan per baris; berpotensi lambat di file besar | `guides/geometry.rs` (catatan `ponytail:`) |

## Catatan Verifikasi

- Batch pertama (A-5, A-6, B-12, B-14, B-16, B-17) telah diimplementasikan Mei 2026 — verifikasi lewat `cargo test` (63 test) yang mencakup `ops.rs` (toggle/duplikat/pindah/hapus) dan `settings.rs` (round-trip recent files via `IONE_CONFIG_DIR`).
- Perilaku Enter/indent & auto-close terverifikasi di kode (`editor/mod.rs`): Enter polos memicu auto-indent (hanya bila leading whitespace > 0), auto-close pasangan kurung/kutip tidak jalan di dalam string/komentar (mask lexer).
- Item yang **sudah terwujud tetapi sempat tercatat sebagai kekurangan** di versi lama `FITUR.md`: **auto-complete ringan** kini tersedia (`completion.rs`, lih. `FITUR.md` → ✨ Auto-Complete) dan **undo/redo kursor tunggal** tersedia via egui bawaan.
- Item roadmap lama `FITUR.md` telah dipetakan ke daftar ini: simpan pengaturan → B-17; git panel → C-21; recent files → B-14; minimap/breadcrumb → C-20; build/run → A-7; LSP/cargo check → A-3.

---

*Rencana diperbarui seiring perkembangan aplikasi. Prioritas bisa berubah berdasarkan kebutuhan pengguna.*