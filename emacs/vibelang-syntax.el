;;; vibelang-syntax.el --- Syntax highlighting for VibeLang -*- lexical-binding: t -*-

;; Copyright (C) 2024 VibeLang Authors
;; Author: VibeLang Team
;; Keywords: languages, music

;;; Commentary:
;; Provides syntax highlighting definitions for VibeLang (.vibe) files.

;;; Code:

(defvar vibelang-keywords
  '("if" "else" "for" "in" "loop" "while" "do" "until"
    "return" "break" "continue" "import" "export" "new" "switch" "throw" "try" "catch")
  "VibeLang keywords.")

(defvar vibelang-storage-types
  '("let" "const" "fn" "private")
  "VibeLang storage type keywords.")

(defvar vibelang-builtins
  '(;; === Global Functions ===
    "set_tempo" "get_tempo"
    "set_time_signature"
    "set_quantization" "get_quantization"
    "get_current_bar"

    ;; === Object Constructors ===
    "voice" "pattern" "melody" "sequence" "group" "define_group"
    "sample" "load_sfz" "modulator" "fx" "record"

    ;; === DSP Definition ===
    "define_synthdef" "define_fx" "define_modulator"

    ;; === Helper Functions: Music Theory ===
    "db" "note" "chord" "scale" "scale_degree" "bars"
    "mtof" "ftom"

    ;; === Helper Functions: Array Utilities ===
    "zip" "shuffle" "rotate" "reverse" "flatten" "repeat" "take" "skip"

    ;; === Helper Functions: Random ===
    "random" "random_range" "random_int" "random_choice" "random_seed"

    ;; === Helper Functions: Math ===
    "clamp" "lerp" "map_range" "smoothstep" "wrap" "quantize"

    ;; === Helper Functions: Type Conversion ===
    "to_int" "to_float" "to_string"

    ;; === Helper Functions: Time ===
    "timestamp" "timestamp_ms"

    ;; === MIDI Functions ===
    "midi_device" "list_midi_devices"

    ;; === DSP UGens (audio rate) ===
    "sin_ar" "saw_ar" "pulse_ar" "tri_ar" "noise_ar"
    "white_noise" "pink_noise" "brown_noise"
    "dust" "impulse" "crackle" "gray_noise"
    "lpf_ar" "hpf_ar" "bpf_ar" "rlpf_ar" "rhpf_ar" "resonz_ar" "moog_ff_ar"
    "delay_ar" "delay_n_ar" "delay_l_ar" "delay_c_ar" "comb_n_ar" "comb_l_ar" "comb_c_ar"
    "allpass_n_ar" "allpass_l_ar" "allpass_c_ar"
    "freeverb_ar" "gverb_ar"
    "compressor_ar" "limiter_ar" "normalizer_ar"
    "distort_ar" "softclip_ar" "tanh_ar" "fold_ar" "wrap_ar" "clip_ar"
    "play_buf" "buf_rd_ar" "loop_buf_ar" "grain_buf_ar"
    "env_gen_ar" "env_perc" "env_adsr" "env_asr" "env_linen" "env_triangle"
    "mix" "pan2" "balance2" "rotate2"
    "out" "in_ar" "local_in" "local_out" "replace_out" "x_out"

    ;; === DSP UGens (control rate) ===
    "sin_kr" "saw_kr" "pulse_kr" "tri_kr" "lf_noise0_kr" "lf_noise1_kr" "lf_noise2_kr"
    "lfo" "env_ar" "line_kr" "x_line_kr"
    "lag_kr" "lag2_kr" "lag3_kr" "slew_kr"
    "in_kr" "out_kr"

    ;; === DSP Utilities ===
    "a2k" "k2a" "dc" "silent"

    ;; === Assertions (for testing) ===
    "assert" "assert_eq" "assert_ne" "assert_gt" "assert_lt"
    "assert_ge" "assert_le" "assert_true" "assert_false"

    ;; === Control Flow Helpers ===
    "print" "debug" "exit")
  "VibeLang built-in functions.")

(defvar vibelang-types
  '("float" "int" "string" "bool")
  "VibeLang type names.")

(defvar vibelang-constants
  '("true" "false")
  "VibeLang constants.")

(defvar vibelang-font-lock-keywords
  `(;; Line comments
    ("//.*$" . font-lock-comment-face)
    ;; Block comments
    ("/\\*\\(?:[^*]\\|\\*[^/]\\)*\\*/" . font-lock-comment-face)
    ;; Keywords
    (,(regexp-opt vibelang-keywords 'words) . font-lock-keyword-face)
    ;; Storage types (let, const, fn)
    (,(regexp-opt vibelang-storage-types 'words) . font-lock-keyword-face)
    ;; Built-in functions
    (,(regexp-opt vibelang-builtins 'words) . font-lock-builtin-face)
    ;; Type names
    (,(regexp-opt vibelang-types 'words) . font-lock-type-face)
    ;; Constants
    (,(regexp-opt vibelang-constants 'words) . font-lock-constant-face)
    ;; Strings (double-quoted)
    ("\"\\(?:[^\"\\\\]\\|\\\\.\\)*\"" . font-lock-string-face)
    ;; Template strings (backtick)
    ("`\\(?:[^`\\\\]\\|\\\\.\\)*`" . font-lock-string-face)
    ;; Numbers (floats and integers)
    ("\\b[0-9]+\\(?:\\.[0-9]+\\)?\\b" . font-lock-constant-face)
    ;; Function calls (identifier followed by paren)
    ("\\b\\([a-zA-Z_][a-zA-Z0-9_]*\\)\\s-*(" 1 font-lock-function-name-face)
    ;; Method calls (.method)
    ("\\.\\([a-zA-Z_][a-zA-Z0-9_]*\\)\\s-*(" 1 font-lock-function-name-face)
    ;; Variable definitions after let/const
    ("\\b\\(?:let\\|const\\)\\s-+\\([a-zA-Z_][a-zA-Z0-9_]*\\)"
     1 font-lock-variable-name-face)
    ;; Function definitions after fn
    ("\\bfn\\s-+\\([a-zA-Z_][a-zA-Z0-9_]*\\)"
     1 font-lock-function-name-face)
    ;; Note names in strings (for melodies)
    ;; Matches: C4, C#5, Db3, C-1 (negative octave), r (rest), R
    ;; Also matches: C4 D4 E4 | F4 G4 A4 (with separators)
    ("\"\\([A-Ga-grR][#b♯♭]?-?[0-9]?\\(?:\\s-\\|[A-Ga-g#b♯♭rR0-9._|-]\\)*\\)\""
     1 'vibelang-notes-pattern-face t)
    ;; Step patterns in strings
    ;; Matches: x, X, o, O (hits), ., _, - (rests), | (bar), 0-9 (velocity)
    ;; Also matches: !, g, >, < (accent/ghost/dynamics)
    ("\"\\([xXoO0-9._|!g><-]+\\(?:\\s-[xXoO0-9._|!g><-]+\\)*\\)\""
     1 'vibelang-step-pattern-face t))
  "Font lock keywords for VibeLang mode.")

;; Custom faces for pattern strings
(defface vibelang-step-pattern-face
  '((t :inherit font-lock-string-face :weight bold))
  "Face for step pattern strings in VibeLang."
  :group 'vibelang-faces)

(defface vibelang-notes-pattern-face
  '((t :inherit font-lock-string-face :slant italic))
  "Face for note pattern strings in VibeLang."
  :group 'vibelang-faces)

(defface vibelang-beat-indicator-face
  '((((class color) (background dark))
     :background "#4a4a00" :foreground "#ffff00")
    (((class color) (background light))
     :background "#ffffcc" :foreground "#666600"))
  "Face for current beat position indicator in VibeLang."
  :group 'vibelang-faces)

(defface vibelang-beat-indicator-active-face
  '((((class color) (background dark))
     :background "#00aa00" :foreground "#ffffff" :weight bold)
    (((class color) (background light))
     :background "#aaffaa" :foreground "#006600" :weight bold))
  "Face for active beat (triggered) position in VibeLang."
  :group 'vibelang-faces)

(defface vibelang-playing-face
  '((((class color) (background dark))
     :background "#003300")
    (((class color) (background light))
     :background "#ccffcc"))
  "Face for playing patterns/melodies in VibeLang."
  :group 'vibelang-faces)

;;; Velocity heat map faces for step patterns

(defface vibelang-velocity-loud-face
  '((((class color) (background dark))
     :foreground "#ff6b6b" :weight bold)
    (((class color) (background light))
     :foreground "#e03131" :weight bold))
  "Face for loud velocity (X) in step patterns."
  :group 'vibelang-faces)

(defface vibelang-velocity-medium-face
  '((((class color) (background dark))
     :foreground "#ffa94d")
    (((class color) (background light))
     :foreground "#e67700"))
  "Face for medium velocity (x) in step patterns."
  :group 'vibelang-faces)

(defface vibelang-velocity-soft-face
  '((((class color) (background dark))
     :foreground "#ffe066")
    (((class color) (background light))
     :foreground "#f59f00"))
  "Face for soft velocity (o) in step patterns."
  :group 'vibelang-faces)

(defface vibelang-velocity-rest-face
  '((((class color) (background dark))
     :foreground "#555555")
    (((class color) (background light))
     :foreground "#aaaaaa"))
  "Face for rests (. _ -) in step patterns."
  :group 'vibelang-faces)

(defface vibelang-velocity-accent-face
  '((((class color) (background dark))
     :foreground "#ff4757" :weight bold :underline t)
    (((class color) (background light))
     :foreground "#c92a2a" :weight bold :underline t))
  "Face for accented notes (!) in step patterns."
  :group 'vibelang-faces)

(defface vibelang-velocity-ghost-face
  '((((class color) (background dark))
     :foreground "#888888" :slant italic)
    (((class color) (background light))
     :foreground "#888888" :slant italic))
  "Face for ghost notes (g) in step patterns."
  :group 'vibelang-faces)

;;; Note name faces with pitch coloring

(defface vibelang-note-c-face
  '((((class color) (min-colors 16777216)) :foreground "#ff6b6b")
    (((class color) (min-colors 256)) :foreground "red")
    (t :foreground "red"))
  "Face for C notes (red)."
  :group 'vibelang-faces)

(defface vibelang-note-d-face
  '((((class color) (min-colors 16777216)) :foreground "#ffa94d")
    (((class color) (min-colors 256)) :foreground "orange")
    (t :foreground "yellow"))
  "Face for D notes (orange)."
  :group 'vibelang-faces)

(defface vibelang-note-e-face
  '((((class color) (min-colors 16777216)) :foreground "#ffe066")
    (((class color) (min-colors 256)) :foreground "yellow")
    (t :foreground "yellow"))
  "Face for E notes (yellow)."
  :group 'vibelang-faces)

(defface vibelang-note-f-face
  '((((class color) (min-colors 16777216)) :foreground "#8ce99a")
    (((class color) (min-colors 256)) :foreground "green")
    (t :foreground "green"))
  "Face for F notes (green)."
  :group 'vibelang-faces)

(defface vibelang-note-g-face
  '((((class color) (min-colors 16777216)) :foreground "#74c0fc")
    (((class color) (min-colors 256)) :foreground "cyan")
    (t :foreground "cyan"))
  "Face for G notes (light blue)."
  :group 'vibelang-faces)

(defface vibelang-note-a-face
  '((((class color) (min-colors 16777216)) :foreground "#9775fa")
    (((class color) (min-colors 256)) :foreground "magenta")
    (t :foreground "magenta"))
  "Face for A notes (purple)."
  :group 'vibelang-faces)

(defface vibelang-note-b-face
  '((((class color) (min-colors 16777216)) :foreground "#f783ac")
    (((class color) (min-colors 256)) :foreground "brightmagenta")
    (t :foreground "magenta"))
  "Face for B notes (pink)."
  :group 'vibelang-faces)

;;; Scale degree faces (rainbow colors for degrees 1-7)
;; Use display-color-cells check for terminal compatibility

(defface vibelang-degree-1-face
  '((((class color) (min-colors 16777216)) :foreground "#ff6b6b" :weight bold)
    (((class color) (min-colors 256)) :foreground "red" :weight bold)
    (t :foreground "red" :weight bold))
  "Face for scale degree 1 (root)."
  :group 'vibelang-faces)

(defface vibelang-degree-2-face
  '((((class color) (min-colors 16777216)) :foreground "#ffa94d" :weight bold)
    (((class color) (min-colors 256)) :foreground "orange" :weight bold)
    (t :foreground "yellow" :weight bold))
  "Face for scale degree 2."
  :group 'vibelang-faces)

(defface vibelang-degree-3-face
  '((((class color) (min-colors 16777216)) :foreground "#ffe066" :weight bold)
    (((class color) (min-colors 256)) :foreground "yellow" :weight bold)
    (t :foreground "yellow" :weight bold))
  "Face for scale degree 3."
  :group 'vibelang-faces)

(defface vibelang-degree-4-face
  '((((class color) (min-colors 16777216)) :foreground "#8ce99a" :weight bold)
    (((class color) (min-colors 256)) :foreground "green" :weight bold)
    (t :foreground "green" :weight bold))
  "Face for scale degree 4."
  :group 'vibelang-faces)

(defface vibelang-degree-5-face
  '((((class color) (min-colors 16777216)) :foreground "#74c0fc" :weight bold)
    (((class color) (min-colors 256)) :foreground "cyan" :weight bold)
    (t :foreground "cyan" :weight bold))
  "Face for scale degree 5."
  :group 'vibelang-faces)

(defface vibelang-degree-6-face
  '((((class color) (min-colors 16777216)) :foreground "#9775fa" :weight bold)
    (((class color) (min-colors 256)) :foreground "magenta" :weight bold)
    (t :foreground "magenta" :weight bold))
  "Face for scale degree 6."
  :group 'vibelang-faces)

(defface vibelang-degree-7-face
  '((((class color) (min-colors 16777216)) :foreground "#f783ac" :weight bold)
    (((class color) (min-colors 256)) :foreground "brightmagenta" :weight bold)
    (t :foreground "magenta" :weight bold))
  "Face for scale degree 7."
  :group 'vibelang-faces)

(defface vibelang-octave-modifier-face
  '((((class color) (background dark))
     :foreground "#888888")
    (((class color) (background light))
     :foreground "#666666"))
  "Face for octave modifiers (' and ,) in scale notation."
  :group 'vibelang-faces)

(defface vibelang-tie-face
  '((((class color) (background dark))
     :foreground "#61afef")
    (((class color) (background light))
     :foreground "#4078f2"))
  "Face for tie markers (-) in melody notation."
  :group 'vibelang-faces)

(defface vibelang-chord-bracket-face
  '((((class color) (background dark))
     :foreground "#c678dd" :weight bold)
    (((class color) (background light))
     :foreground "#a626a4" :weight bold))
  "Face for chord brackets [] in melody notation."
  :group 'vibelang-faces)

(defface vibelang-chord-suffix-face
  '((((class color) (background dark))
     :foreground "#98c379" :slant italic)
    (((class color) (background light))
     :foreground "#50a14f" :slant italic))
  "Face for chord suffixes (:m7, :maj7, etc.) in melody notation."
  :group 'vibelang-faces)

(defface vibelang-bar-separator-face
  '((((class color) (background dark))
     :foreground "#5c6370")
    (((class color) (background light))
     :foreground "#a0a1a7"))
  "Face for bar separators (|) in patterns."
  :group 'vibelang-faces)

;;; Velocity highlighting in pattern strings

(defun vibelang--colorize-steps (start end)
  "Colorize step characters from START to END with velocity faces."
  (save-excursion
    (goto-char start)
    (while (< (point) end)
      (let ((char (char-after)))
        (cond
         ;; Loud - uppercase X or numbers 7-9
         ((memq char '(?X ?7 ?8 ?9))
          (put-text-property (point) (1+ (point))
                             'font-lock-face 'vibelang-velocity-loud-face))
         ;; Medium - lowercase x or numbers 4-6
         ((memq char '(?x ?4 ?5 ?6))
          (put-text-property (point) (1+ (point))
                             'font-lock-face 'vibelang-velocity-medium-face))
         ;; Soft - o or numbers 1-3
         ((memq char '(?o ?1 ?2 ?3))
          (put-text-property (point) (1+ (point))
                             'font-lock-face 'vibelang-velocity-soft-face))
         ;; Open/strong hit - uppercase O
         ((eq char ?O)
          (put-text-property (point) (1+ (point))
                             'font-lock-face 'vibelang-velocity-loud-face))
         ;; Accent / dynamics up
         ((memq char '(?! ?>))
          (put-text-property (point) (1+ (point))
                             'font-lock-face 'vibelang-velocity-accent-face))
         ;; Ghost / dynamics down
         ((memq char '(?g ?<))
          (put-text-property (point) (1+ (point))
                             'font-lock-face 'vibelang-velocity-ghost-face))
         ;; Rest
         ((memq char '(?. ?_ ?- ?0))
          (put-text-property (point) (1+ (point))
                             'font-lock-face 'vibelang-velocity-rest-face))))
      (forward-char))))

(defun vibelang--colorize-notes (start end)
  "Colorize melody notation from START to END.

Supports all VibeLang melody notation:
- Scale degrees: 1-7 with octave modifiers (' for up, , for down)
- Absolute notes: C4, D#5, Eb3, etc.
- Chords: [C4 E4 G4] or C4:m7, D4:maj7
- Ties: - (hold previous note)
- Rests: . or _ (silence)
- Bar separators: |"
  (save-excursion
    (goto-char start)
    (while (< (point) end)
      (let ((char (char-after)))
        (cond
         ;; Scale degrees 1-7
         ((and char (>= char ?1) (<= char ?7))
          (let ((degree-start (point))
                (degree (- char ?0)))
            (forward-char)
            ;; Collect octave modifiers: ' (up) or , (down)
            (let ((modifier-start (point)))
              (while (and (< (point) end)
                          (let ((c (char-after)))
                            (or (eq c ?') (eq c ?,))))
                (forward-char))
              ;; Colorize the degree number
              (put-text-property degree-start (1+ degree-start)
                                 'font-lock-face
                                 (vibelang--degree-face degree))
              ;; Colorize the octave modifiers if any
              (when (> (point) modifier-start)
                (put-text-property modifier-start (point)
                                   'font-lock-face 'vibelang-octave-modifier-face)))))

         ;; Absolute note names A-G (with optional accidentals, octave, and chord suffix)
         ((and char (or (and (>= char ?A) (<= char ?G))
                        (and (>= char ?a) (<= char ?g))))
          (let ((note-start (point))
                (note-char (upcase char)))
            (forward-char)
            ;; Collect accidentals: # b ♯ ♭
            (while (and (< (point) end)
                        (let ((c (char-after)))
                          (memq c '(?# ?b ?♯ ?♭))))
              (forward-char))
            ;; Collect octave: optional minus and digits
            (when (and (< (point) end) (eq (char-after) ?-))
              (forward-char))
            (while (and (< (point) end)
                        (let ((c (char-after)))
                          (and c (>= c ?0) (<= c ?9))))
              (forward-char))
            ;; Colorize the note
            (let ((note-end (point)))
              (put-text-property note-start note-end
                                 'font-lock-face
                                 (vibelang--note-face note-char))
              ;; Check for chord suffix: :m7, :maj7, :dim, etc.
              (when (and (< (point) end) (eq (char-after) ?:))
                (let ((suffix-start (point)))
                  (forward-char) ; skip :
                  ;; Collect alphanumeric chord suffix
                  (while (and (< (point) end)
                              (let ((c (char-after)))
                                (and c (or (and (>= c ?a) (<= c ?z))
                                           (and (>= c ?A) (<= c ?Z))
                                           (and (>= c ?0) (<= c ?9))))))
                    (forward-char))
                  (put-text-property suffix-start (point)
                                     'font-lock-face 'vibelang-chord-suffix-face))))))

         ;; Chord brackets
         ((and char (eq char ?\[))
          (put-text-property (point) (1+ (point))
                             'font-lock-face 'vibelang-chord-bracket-face)
          (forward-char))
         ((and char (eq char ?\]))
          (put-text-property (point) (1+ (point))
                             'font-lock-face 'vibelang-chord-bracket-face)
          (forward-char))

         ;; Tie marker
         ((and char (eq char ?-))
          (put-text-property (point) (1+ (point))
                             'font-lock-face 'vibelang-tie-face)
          (forward-char))

         ;; Rest markers (. and _)
         ((and char (or (eq char ?.) (eq char ?_)))
          (put-text-property (point) (1+ (point))
                             'font-lock-face 'vibelang-velocity-rest-face)
          (forward-char))

         ;; Bar separator
         ((and char (eq char ?|))
          (put-text-property (point) (1+ (point))
                             'font-lock-face 'vibelang-bar-separator-face)
          (forward-char))

         ;; Rest note: r or R
         ((and char (or (eq char ?r) (eq char ?R)))
          (put-text-property (point) (1+ (point))
                             'font-lock-face 'vibelang-velocity-rest-face)
          (forward-char))

         ;; Whitespace and other characters - skip
         (t (forward-char)))))))

(defun vibelang--degree-face (degree)
  "Return the face for scale DEGREE (1-7)."
  (pcase degree
    (1 'vibelang-degree-1-face)
    (2 'vibelang-degree-2-face)
    (3 'vibelang-degree-3-face)
    (4 'vibelang-degree-4-face)
    (5 'vibelang-degree-5-face)
    (6 'vibelang-degree-6-face)
    (7 'vibelang-degree-7-face)
    (_ 'vibelang-notes-pattern-face)))

(defun vibelang--note-face (note-char)
  "Return the face for NOTE-CHAR (A-G uppercase, or R for rest)."
  (pcase note-char
    (?C 'vibelang-note-c-face)
    (?D 'vibelang-note-d-face)
    (?E 'vibelang-note-e-face)
    (?F 'vibelang-note-f-face)
    (?G 'vibelang-note-g-face)
    (?A 'vibelang-note-a-face)
    (?B 'vibelang-note-b-face)
    (?R 'vibelang-velocity-rest-face)
    (_ 'vibelang-notes-pattern-face)))

;; Add jit-lock fontification for pattern colorization
(defun vibelang-syntax-propertize (start end)
  "Apply syntax properties from START to END."
  (save-excursion
    (goto-char start)
    ;; Find and colorize step patterns
    (while (re-search-forward "\\.step(\"\\([^\"]+\\)\")" end t)
      (vibelang--colorize-steps (match-beginning 1) (match-end 1)))
    ;; Find and colorize note patterns
    (goto-char start)
    (while (re-search-forward "\\.notes(\"\\([^\"]+\\)\")" end t)
      (vibelang--colorize-notes (match-beginning 1) (match-end 1)))))

(provide 'vibelang-syntax)
;;; vibelang-syntax.el ends here
