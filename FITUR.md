# ione — Fitur yang Sudah Diimplementasikan

> Editor kode modern berbasis **Rust + egui/eframe** dengan terminal PowerShell terintegrasi.
> Dokumen ini hanya mencatat fitur yang **saat ini tersedia**. Fitur yang belum ada / rencana update dicatat di **[`fitur-up.md`](fitur-up.md)**.

---

## Daftar Isi

- [Cara Membuka](#cara-membuka)
- [Fitur Utama](#fitur-utama)
- [Kelebihan](#kelebihan)
- [Catatan](#catatan)

---

## Cara Membuka

| Aksi | Menu | Shortcut |
|------|------|----------|
| Buka file | File → Open... | `Ctrl+O` |
| Buka folder | File → Open Folder... | `Ctrl+K Ctrl+O` |
| File baru | File → New File | `Ctrl+N` |
| Quick open | File → Quick Open | `Ctrl+P` |
| Buka file terakhir | File → Open Recent (10 terakhir) | — |
| Simpan | File → Save | `Ctrl+S` |
| Simpan sebagai | File → Save As... | `Ctrl+Shift+S` |
| Tutup tab | File → Close Tab | `Ctrl+W` |

---

## Fitur Utama

### 📝 Editor Multi-Tab
- Buka banyak file sekaligus dalam tab.
- **Syntax highlighting** untuk banyak bahasa (Rust, Python, JS/TS, HTML/CSS, JSON, TOML, YAML, Markdown, Lua, Shell, SQL, C, dan lainnya).
- **Nomor baris**, **undo/redo** (kursor tunggal, bawaan egui), **clickable links**.
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
- **Tahan kedalaman tak terbatas:** indentasi baris dibatasi otomatis sehingga pohon sesedalam apa pun tetap tampil rapi — tidak ada baris yang pernah "hilang" meski foldernya sangat dalam.
- **Garis pemisah permanen** di tepi kanan explorer (antara explorer ↔ editor) digambar di lapisan teratas, jadi selalu terlihat — berapa pun isi/tinggi pohon atau status scroll.

### 🔍 Find & Replace
- Pencarian teks dengan hasil **live**.
- Ganti satu per satu (`FindNext/Prev`) atau **ganti semua** (`ReplaceAll`).
- Mendukung pencarian case-sensitive.
- Dipanggil dengan `Ctrl+H`.

### 🗂️ Outline Panel
- Menampilkan struktur file aktif untuk navigasi cepat.

### 📐 Code Folding
- Lipat blok `{ … }` dengan **`Ctrl+Shift+[`** (lipat) / **`Ctrl+Shift+]`** (buka).
- Hanya blok yang layak dilipat (fn body, if/else) — literal ekspresi multi-baris (mis. `Foo { … };`) tidak dianggap.
- Interior yang tersembunyi tampil sebagai `⋯` di baris pembuka; mengetik tepat di marker otomatis membuka lipatan.
- Ikon chevron SVG di gutter: `^` untuk membuka lipatan, `v` untuk menutup blok.

### ✍️ Multi-Select Edit (Ctrl+D)
- `Ctrl+D` memilih kejadian berikutnya dari kata di bawah kursor.
- Lalu **satu ketikan/backspace/delete/tab dipakai di semua lokasi sekaligus**.
- Klik / tombol panah / Home / End membatalkan mode.
- Overlay highlight biru transparan menandai rentang terpilih.

### ⌨️ Pengetikan Cepat
- **Auto-close pasangan**: `(` → `()`, `[` → `[]`, `{` → `{}`, `"` → `""`, `'` → `''`, `` ` `` → backtick tersambung — kursor langsung berada **di dalam** pasangan.
- **Auto-indent**: `Enter` membawa indentasi (whitespace) baris sebelumnya ke baris baru.
- Cerdas konteks: kedua hal di atas **dilewati** saat kursor berada di dalam string/komentar atau bersebelahan dengan karakter kata, dan tidak aktif saat ada seleksi / mode multi-cursor.
- **Operasi baris** (buffernya tetap utuh):
  - `Ctrl+/` — toggle komentar baris (`//`; `#` untuk CSV/terminal), sekaligus untuk seleksi multi-baris, baris kosong dilewati.
  - `Ctrl+Shift+D` — duplikat baris (kursor ikut ke salinan).
  - `Ctrl+Shift+K` — hapus baris.
  - `Alt+↑` / `Alt+↓` — pindahkan baris ke atas/bawah.
- **Pengingat**: fitur di atas menargetkan kursor tunggal; mode multi-cursor (`Ctrl+D`) tidak dijinakkan olehnya.

### ⚙️ Pengaturan Persisten
- **Tema & font editor tersimpan antar sesi** di `%APPDATA%\ione\settings.txt` dan diterapkan otomatis saat aplikasi dibuka.
- **Daftar recent files** (10 terakhir) tersimpan di `%APPDATA%\ione\recent.txt`, tampil di **File → Open Recent**, dan ter-update setiap kali membuka file (via Ctrl+P atau explorer).

### ✨ Auto-Complete
- Saran muncul saat mengetik, dari 3 sumber lokal:
  - **Kata kunci** syntax bahasa aktif (mis. `fn`, `let`, `return` untuk Rust).
  - **Simbol** dari outline file aktif — dengan petunjuk parameter untuk fungsi (signature hint).
  - **Identifier** yang pernah dipakai di file itu sendiri.
- Selesai tanpa proses eksternal (offline, tanpa LSP).
- Modus multi-cursor berjalan bersama auto-complete tanpa konflik.

### 🛑 Error/Warning Decoration
- Deteksi bawaan (tanpa proses eksternal, offline):
  - Kurung tidak berpasangan / kurung tutup nyasar → **error**.
  - **Trailing whitespace** → warning.
  - **Indentasi campuran tab+spasi** → warning.
- Ditampilkan sebagai **squiggle `~~~~~`** (merah = error, kuning = warning), **tooltip** saat hover, **marker** di gutter, dan **hitungan** di status bar.
- Analisis hanya dijalankan ulang saat konten berubah; rentang tersembunyi di balik fold otomatis dilewati.
- Pemeriksaan kurung **akurat per bahasa** — tidak ada lagi *error palsu* ("unbalanced" disebabkan teks biasa):
  - **JavaScript / TypeScript — termasuk JSX & TSX:** literal *template string* `` `…${…}` `` dan *regex literal* `/…/` dikenali dengan benar. Isi `${…}` **tetap** diperiksa kurungnya; sebaliknya regex beserta quantifier-nya (`/a{2,3}/`, `/[{}(]/`) tidak pernah dianggap kurung hilang. Pembagian (`a / b`) vs regex dibedakan lewat konteks pemanggilan.
  - **Rust:** string `"…"`, *raw string* `r#"…"#`, char literal `'a'`, dan *lifetime* `'a` dilewati — apostrof pada lifetime tidak lagi memicu error palsu.
  - **Bahasa lain** (Python, C/C++, HTML/CSS, JSON, TOML, dsb.): scanner generik melewati string & komentar dengan benar.

### 🎯 Bracket Guides & Rainbow Brackets
- Panah panduan vertikal untuk pasangan kurung multi-baris (bisa di-hover).
- Rainbow brackets: warna berbeda per kedalaman nesting.
- Dikelola via **View → Editor Guides** (bisa dimatikan).

### 🎨 Tema
- **Dark (hitam murni)** dan **Light (putih)**.
- Semua sudut **tajam** (tanpa radius), mengikuti estetika bersih modern.
- **GitHub Light masih *beta*** — ditandai `(beta)` di menu **View → Theme**; muncul **konfirmasi** saat memilihnya karena sebagian warna/aksen bisa berubah di versi berikutnya.

### 🔠 Ganti Font Editor
- **View → Editor Font**: pilih di antara 5 font coding gratis:
  JetBrains Mono, Fira Code, Source Code Pro, IBM Plex Mono, Roboto Mono.
- Berlaku untuk **canvas editor + terminal**.
- Font aktif ditandai ✓; font rusak/missing → pesan aman, tidak crash.

### ✨ Lainnya
- **Splash screen** animasi saat aplikasi dibuka (+ overlay ringan saat membuka file besar).
- **Diagnostik bawaan** (error/warning squiggle) dan **multi-select edit** — tanpa dependency eksternal.
- Ikon SVG/berkualitas tinggi untuk file menurut jenisnya.

---

## Kelebihan

- ✅ **Ringan & cepat startup**: berbasis egui/eframe, tidak memakan banyak memori seperti IDE besar.
- ✅ **Terminal PowerShell bawaan** yang benar-benar fungsional (bukan emulasi), multi-session.
- ✅ **UI tajam & konsisten** dengan dua tema (hitam/putih) yang bersih.
- ✅ **Bisa ganti font** untuk pengalaman coding yang nyaman.
- ✅ **Open source penuh kontrol**, kode sederhana dan mudah dikembangkan.
- ✅ **Self-contained**: tidak butuh ekstensi rumit untuk fitur inti.
- ✅ **Deteksi error anti-palsu**: parser sadar bahasa (JS/TS/JSX/TSX template & regex, lifetime/raw-string Rust) — yang dilaporkan hanya error sungguhan, bukan teks biasa yang salah dibaca.

---

## Catatan

- Dokumen ini hanya mencatat fitur yang **sudah diimplementasikan**.
- Fitur yang **belum ada** (kekurangan) dan **rencana update**: lihat **[`fitur-up.md`](fitur-up.md)**.

---

*Dokumen diperbarui sesuai kondisi aplikasi saat ini. Beberapa fitur masih dalam pengembangan aktif.*
