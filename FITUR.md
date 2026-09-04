# ione — Penjelasan Fitur

> Editor kode modern berbasis **Rust + egui/eframe** dengan terminal PowerShell terintegrasi.
> Dokumen ini menjelaskan fitur yang **saat ini tersedia**, lengkap dengan kelebihan dan kekurangannya.

---

## Daftar Isi

- [Cara Membuka](#cara-membuka)
- [Fitur Utama](#fitur-utama)
- [Kelebihan](#kelebihan)
- [Kekurangan](#kekurangan)
- [Roadmap / Rencana Perbaikan](#roadmap)

---

## Cara Membuka

| Aksi | Menu | Shortcut |
|------|------|----------|
| Buka file | File → Open... | `Ctrl+O` |
| Buka folder | File → Open Folder... | `Ctrl+K Ctrl+O` |
| File baru | File → New File | `Ctrl+N` |
| Simpan | File → Save | `Ctrl+S` |
| Simpan sebagai | File → Save As... | `Ctrl+Shift+S` |
| Tutup tab | File → Close Tab | `Ctrl+W` |

---

## Fitur Utama

### 📝 Editor Multi-Tab
- Buka banyak file sekaligus dalam tab.
- **Syntax highlighting** untuk banyak bahasa (Rust, Python, JS/TS, HTML/CSS, JSON, TOML, YAML, Markdown, Lua, Shell, SQL, C, dan lainnya).
- **Nomor baris**, **undo/redo**, **clickable links**.
- Status bar menampilkan posisi kursor (baris & kolom).
- Beralih tab cepat dengan `Ctrl+Tab` / `Ctrl+Shift+Tab`.
- Tutup tab dengan **klik tengah** atau **klik kanan → Close** (sama seperti terminal).

### 🖥️ Terminal PowerShell Multi-Session
- Buka **banyak sesi terminal sekaligus**, masing-masing berjalan mandiri.
- Buat sesi baru dengan tombol **+**.
- Pindah antar-sesi dengan **klik kiri** pada tab.
- Tutup sesi dengan **klik tengah** atau **klik kanan → Close** (minimal 1 sesi).
- Tampil/menghilang dengan `Ctrl+`` ` (backtick).

### 📂 File Explorer
- Menampilkan struktur folder secara **pohon** (expand/collapse).
- Klik ganda/ikon untuk membuka file.
- Klik kanan untuk menu: **Open, Rename, Delete, Copy Path**.
- Auto-refresh saat folder dibuka.

### 🔍 Find & Replace
- Pencarian teks dengan hasil **live**.
- Ganti satu per satu (`FindNext/Prev`) atau **ganti semua** (`ReplaceAll`).
- Mendukung pencarian case-sensitive.
- Dipanggil dengan `Ctrl+H`.

### 🗂️ Outline Panel
- Menampilkan struktur file aktif untuk navigasi cepat.

### 🎨 Tema
- **Dark (hitam murni)** dan **Light (putih)**.
- Semua sudut **tajam** (tanpa radius), mengikuti estetika bersih modern.

### 🔠 Ganti Font Editor
- **View → Editor Font**: pilih di antara 5 font coding gratis:
  JetBrains Mono, Fira Code, Source Code Pro, IBM Plex Mono, Roboto Mono.
- Berlaku untuk **canvas editor + terminal**.
- Font aktif ditandai ✓; font rusak/missing → pesan aman, tidak crash.

### ✨ Lainnya
- **Splash screen** animasi saat aplikasi dibuka.
- **About window** (Help → About) berisi info lengkap, shortcut, dan tech stack.
- Ikon SVG/berkualitas tinggi untuk file menurut jenisnya.

---

## Kelebihan

- ✅ **Ringan & cepat startup**: berbasis egui/eframe, tidak memakan banyak memori seperti IDE besar.
- ✅ **Terminal PowerShell bawaan** yang benar-benar fungsional (bukan emulasi), multi-session.
- ✅ **UI tajam & konsisten** dengan dua tema (hitam/putih) yang bersih.
- ✅ **Bisa ganti font** untuk pengalaman coding yang nyaman.
- ✅ **Open source penuh kontrol**, kode sederhana dan mudah dikembangkan.
- ✅ **Self-contained**: tidak butuh ekstensi rumit untuk fitur inti.

---

## Kekurangan

- ⚠️ **Belum ada auto-complete / IntelliSense** di editor (kecuali melalui terminal).
- ⚠️ **Belum ada debugger, git panel, atau integrasi build/run moderen**.
- ⚠️ **Manajemen proyek masih dasar**: belum ada "workspace" multi-root atau pengaturan per-proyek.
- ⚠️ **Roboto Mono** hanya satu varian (Regular) karena format variable font.
- ⚠️ **Belum ada pengaturan/prference tersimpan** (misal font & tema pilihan belum dipertahankan antar sesi).
- ⚠️ **Belum ada penyorotan error/inlay, minimap, atau breadcrumb**.
- ⚠️ **Terminal belum mendukung beberapa fitur lanjutan** (misal drag-drop file ke terminal).

---

## Roadmap

| Prioritas | Rencana |
|-----------|---------|
| 🥇 | Simpan pengaturan (font, tema, ukuran) agar persisten |
| 🥇 | Auto-complete dasar untuk bahasa populer |
| 🥈 | Panel Git (status, commit, staging) |
| 🥈 | Persist daftar file yang baru dibuka (recent files) |
| 🥉 | Minimap & breadcrumb |
| 🥉 | Integrasi build/run dengan output terarah ke terminal |

---

*Dokumen diperbarui sesuai kondisi aplikasi saat ini. Beberapa fitur masih dalam pengembangan aktif.*
