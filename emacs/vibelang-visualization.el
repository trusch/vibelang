;;; vibelang-visualization.el --- Beat indicators and overlays -*- lexical-binding: t -*-

;; Copyright (C) 2024 VibeLang Authors
;; Author: VibeLang Team
;; Keywords: multimedia, music

;;; Commentary:
;; Provides real-time visualization of playback position in VibeLang buffers.
;; Shows beat indicators in pattern, melody, and sequence strings, and highlights
;; currently playing elements including active clips within sequences.
;;
;; Architecture:
;; - Server sends playback.tick events every 16th note (~8 updates per beat)
;; - Each tick triggers overlay updates via vibelang-viz--update-all
;; - Beat overlays are reused (moved) rather than recreated for smoothness
;; - No client-side interpolation - server ticks are frequent enough

;;; Code:

(require 'vibelang-syntax)

;;; Buffer-local state

(defvar-local vibelang-viz--pattern-info nil
  "Alist of (name . info) for pattern/melody/sequence visualization.
Each info is a plist with :start, :end, :slots, :type, :slot-count, :length.
For sequences, also includes :clips with parsed clip info.")

(defvar-local vibelang-viz--enabled t
  "Whether visualization is enabled in this buffer.")

(defvar-local vibelang-viz--last-scan-tick nil
  "Buffer modification tick of last pattern scan.")

(defvar-local vibelang-viz--last-playback-data nil
  "Last received playback data.")

(defvar-local vibelang-viz--last-update-time nil
  "Time of last playback data update.")

(defvar-local vibelang-viz--transport nil
  "Last received transport data (bpm, beat, etc).")

(defvar-local vibelang-viz--change-debounce-timer nil
  "Timer for debouncing buffer change handling.")

;;; Overlay reuse tables - for smooth updates without flicker

(defvar-local vibelang-viz--beat-overlays nil
  "Hash table mapping entity key to its beat indicator overlay.
Overlays are moved rather than recreated for smooth animation.")

(defvar-local vibelang-viz--playing-overlays nil
  "Hash table mapping entity key to its playing line overlay.")

;;; Faces for visualization

(defface vibelang-clip-active-face
  '((((class color) (background dark))
     :background "#1a3a1a" :extend t)
    (((class color) (background light))
     :background "#e0ffe0" :extend t))
  "Face for active clips in sequences."
  :group 'vibelang-faces)

(defface vibelang-clip-progress-face
  '((((class color) (background dark))
     :foreground "#00ff00" :weight bold)
    (((class color) (background light))
     :foreground "#006600" :weight bold))
  "Face for clip progress indicator."
  :group 'vibelang-faces)

;;; Minor mode

(define-minor-mode vibelang-visualization-mode
  "Minor mode for VibeLang playback visualization.
Shows beat position indicators in pattern/melody/sequence strings."
  :lighter " Viz"
  :group 'vibelang
  (if vibelang-visualization-mode
      (vibelang-viz--setup)
    (vibelang-viz--teardown)))

(defun vibelang-viz--setup ()
  "Set up visualization for current buffer."
  (add-hook 'after-change-functions #'vibelang-viz--on-buffer-change nil t)
  (setq vibelang-viz--beat-overlays (make-hash-table :test 'equal))
  (setq vibelang-viz--playing-overlays (make-hash-table :test 'equal))
  (vibelang-viz--scan-patterns))

(defun vibelang-viz--teardown ()
  "Tear down visualization for current buffer."
  (when vibelang-viz--change-debounce-timer
    (cancel-timer vibelang-viz--change-debounce-timer)
    (setq vibelang-viz--change-debounce-timer nil))
  (vibelang-viz--clear-all-overlays)
  (setq vibelang-viz--beat-overlays nil)
  (setq vibelang-viz--playing-overlays nil)
  (remove-hook 'after-change-functions #'vibelang-viz--on-buffer-change t))

(defun vibelang-viz--on-buffer-change (&rest _)
  "Handle buffer changes - mark patterns for rescan."
  (setq vibelang-viz--last-scan-tick nil)
  (when vibelang-viz--change-debounce-timer
    (cancel-timer vibelang-viz--change-debounce-timer))
  (setq vibelang-viz--change-debounce-timer
        (run-with-timer 0.1 nil #'vibelang-viz--clear-stale-data)))

(defun vibelang-viz--clear-stale-data ()
  "Clear cached playback data after buffer changes."
  (setq vibelang-viz--change-debounce-timer nil)
  (setq vibelang-viz--last-playback-data nil)
  (setq vibelang-viz--last-update-time nil)
  (vibelang-viz--clear-all-overlays))

;;; Overlay management - key to smooth animation

(defun vibelang-viz--get-or-move-beat-overlay (key pos len)
  "Get beat overlay for KEY, creating if needed. Move to POS with LEN."
  (unless vibelang-viz--beat-overlays
    (setq vibelang-viz--beat-overlays (make-hash-table :test 'equal)))
  (let ((ov (gethash key vibelang-viz--beat-overlays)))
    (if (and ov (overlay-buffer ov))
        ;; Move existing overlay - this is smooth, no flicker
        (move-overlay ov pos (+ pos len))
      ;; Create new overlay
      (setq ov (make-overlay pos (+ pos len)))
      (overlay-put ov 'face 'vibelang-beat-indicator-active-face)
      (overlay-put ov 'vibelang-overlay t)
      (overlay-put ov 'vibelang-beat-overlay t)
      (overlay-put ov 'priority 100)
      (puthash key ov vibelang-viz--beat-overlays))
    ov))

(defun vibelang-viz--remove-beat-overlay (key)
  "Remove beat overlay for KEY if it exists."
  (when vibelang-viz--beat-overlays
    (let ((ov (gethash key vibelang-viz--beat-overlays)))
      (when (and ov (overlay-buffer ov))
        (delete-overlay ov))
      (remhash key vibelang-viz--beat-overlays))))

(defun vibelang-viz--add-playing-overlay (start end)
  "Add playing overlay from START to END (line highlight)."
  (save-excursion
    (goto-char start)
    (let* ((line-start (line-beginning-position))
           (line-end (line-end-position))
           (ov (make-overlay line-start line-end)))
      (overlay-put ov 'face 'vibelang-playing-face)
      (overlay-put ov 'vibelang-overlay t)
      (overlay-put ov 'vibelang-playing-overlay t)
      (overlay-put ov 'priority 50))))

(defun vibelang-viz--clear-playing-overlays ()
  "Clear all playing line overlays."
  (remove-overlays (point-min) (point-max) 'vibelang-playing-overlay t))

(defun vibelang-viz--clear-clip-overlays ()
  "Clear all clip/sequence overlays."
  (remove-overlays (point-min) (point-max) 'vibelang-clip-overlay t))

(defun vibelang-viz--clear-all-overlays ()
  "Clear all visualization overlays."
  (remove-overlays (point-min) (point-max) 'vibelang-overlay t)
  (when vibelang-viz--beat-overlays
    (clrhash vibelang-viz--beat-overlays)))

;;; String parsing helpers

(defun vibelang-viz--find-string-bounds ()
  "Find the bounds of a string at or after point.
Returns (START END) or nil."
  (skip-chars-forward " \t\n")
  (cond
   ((eq (char-after) ?`)
    (vibelang-viz--parse-backtick-string))
   ((looking-at "#+\"")
    (vibelang-viz--parse-raw-string))
   ((eq (char-after) ?\")
    (vibelang-viz--parse-double-quoted-string))
   (t nil)))

(defun vibelang-viz--parse-double-quoted-string ()
  "Parse a double-quoted string at point."
  (when (eq (char-after) ?\")
    (let ((string-start (1+ (point))))
      (forward-char 1)
      (while (and (not (eobp))
                  (not (eq (char-after) ?\")))
        (if (eq (char-after) ?\\)
            (forward-char 2)
          (forward-char 1)))
      (when (eq (char-after) ?\")
        (list string-start (point))))))

(defun vibelang-viz--parse-backtick-string ()
  "Parse a backtick-quoted string at point."
  (when (eq (char-after) ?`)
    (let ((string-start (1+ (point))))
      (forward-char 1)
      (while (and (not (eobp))
                  (not (eq (char-after) ?`)))
        (forward-char 1))
      (when (eq (char-after) ?`)
        (list string-start (point))))))

(defun vibelang-viz--parse-raw-string ()
  "Parse a raw string #\"...\"# at point."
  (when (looking-at "\\(#+\\)\"")
    (let* ((hashes (match-string 1))
           (hash-count (length hashes))
           (string-start (match-end 0))
           (close-pattern (concat "\"" (make-string hash-count ?#))))
      (goto-char string-start)
      (when (search-forward close-pattern nil t)
        (list string-start (- (point) (1+ hash-count)))))))

(defun vibelang-viz--find-statement-end (start)
  "Find the end of statement starting at START."
  (save-excursion
    (goto-char start)
    (if (re-search-forward ";" nil t)
        (point)
      (point-max))))

(defun vibelang-viz--in-comment-p (pos)
  "Check if POS is inside a comment."
  (save-excursion
    (goto-char pos)
    (let ((state (parse-partial-sexp (point-min) pos)))
      (nth 4 state))))

;;; Pattern scanning

(defun vibelang-viz--scan-patterns ()
  "Scan buffer for pattern/melody/sequence definitions."
  (when (or (null vibelang-viz--last-scan-tick)
            (not (equal vibelang-viz--last-scan-tick (buffer-modified-tick))))
    (setq vibelang-viz--pattern-info nil)
    (save-excursion
      ;; Find patterns
      (goto-char (point-min))
      (while (re-search-forward "pattern(\"\\([^\"]+\\)\")" nil t)
        (let ((name (match-string 1))
              (pattern-start (match-beginning 0)))
          (save-excursion
            (let ((limit (vibelang-viz--find-statement-end pattern-start))
                  (found nil))
              (goto-char pattern-start)
              (while (and (not found)
                          (re-search-forward "\\.step(" limit t))
                (unless (vibelang-viz--in-comment-p (match-beginning 0))
                  (let ((bounds (vibelang-viz--find-string-bounds)))
                    (when bounds
                      (let* ((step-start (nth 0 bounds))
                             (step-end (nth 1 bounds))
                             (step-string (buffer-substring-no-properties
                                           step-start step-end)))
                        (vibelang-viz--register-pattern
                         name step-start step-end step-string)
                        (setq found t))))))))))

      ;; Find melodies
      (goto-char (point-min))
      (while (re-search-forward "melody(\"\\([^\"]+\\)\")" nil t)
        (let ((name (match-string 1))
              (melody-start (match-beginning 0)))
          (save-excursion
            (let ((limit (vibelang-viz--find-statement-end melody-start))
                  (found nil))
              (goto-char melody-start)
              (while (and (not found)
                          (re-search-forward "\\.notes(" limit t))
                (unless (vibelang-viz--in-comment-p (match-beginning 0))
                  (let ((bounds (vibelang-viz--find-string-bounds)))
                    (when bounds
                      (let* ((notes-start (nth 0 bounds))
                             (notes-end (nth 1 bounds))
                             (notes-string (buffer-substring-no-properties
                                            notes-start notes-end)))
                        (vibelang-viz--register-melody
                         name notes-start notes-end notes-string)
                        (setq found t))))))))))

      ;; Find sequences
      (goto-char (point-min))
      (while (re-search-forward "sequence(\"\\([^\"]+\\)\")" nil t)
        (let ((name (match-string 1))
              (seq-start (match-beginning 0)))
          (unless (vibelang-viz--preceded-by-comma-p seq-start)
            (when (looking-at "[ \t\n]*\\.")
              (save-excursion
                (let ((limit (vibelang-viz--find-statement-end seq-start)))
                  (when (re-search-forward "\\.\\(start\\|apply\\)()" limit t)
                    (vibelang-viz--register-sequence name seq-start (point) limit)))))))))
    (setq vibelang-viz--last-scan-tick (buffer-modified-tick))))

(defun vibelang-viz--preceded-by-comma-p (pos)
  "Check if POS is preceded by a comma."
  (save-excursion
    (goto-char pos)
    (skip-chars-backward " \t\n")
    (and (> (point) (point-min))
         (eq (char-before) ?,))))

(defun vibelang-viz--register-pattern (name start end pattern-string)
  "Register pattern NAME at START to END with PATTERN-STRING."
  (let* ((parse-result (vibelang-viz--parse-pattern-slots pattern-string start))
         (slots (car parse-result))
         (length (cdr parse-result)))
    (push (cons name (list :start start :end end :slots slots
                           :type 'pattern :slot-count (length slots) :length length))
          vibelang-viz--pattern-info)))

(defun vibelang-viz--register-melody (name start end notes-string)
  "Register melody NAME at START to END with NOTES-STRING."
  (let* ((parse-result (vibelang-viz--parse-notes-slots notes-string start))
         (slots (car parse-result))
         (length (cdr parse-result)))
    (push (cons name (list :start start :end end :slots slots
                           :type 'melody :slot-count (length slots) :length length))
          vibelang-viz--pattern-info)))

(defun vibelang-viz--register-sequence (name start end limit)
  "Register sequence NAME from START to END, parsing clips up to LIMIT."
  (let ((clips (vibelang-viz--parse-sequence-clips start limit)))
    (push (cons name (list :start start :end end :slots nil :type 'sequence :clips clips))
          vibelang-viz--pattern-info)))

(defun vibelang-viz--parse-sequence-clips (start limit)
  "Parse .clip() calls between START and LIMIT."
  (let ((clips '())
        (index 0))
    (save-excursion
      (goto-char start)
      (while (re-search-forward "\\.clip(" limit t)
        (let* ((clip-start (match-beginning 0))
               (paren-start (1- (point)))
               (clip-end (vibelang-viz--find-matching-paren paren-start)))
          (when clip-end
            (push (list index clip-start clip-end) clips)
            (goto-char clip-end)
            (setq index (1+ index))))))
    (nreverse clips)))

(defun vibelang-viz--find-matching-paren (open-pos)
  "Find matching close paren for OPEN-POS."
  (save-excursion
    (goto-char open-pos)
    (condition-case nil
        (progn (forward-sexp) (point))
      (scan-error nil))))

(defun vibelang-viz--parse-pattern-slots (pattern-string start-pos)
  "Parse PATTERN-STRING from START-POS. Return (slots . total-length)."
  (let ((slots '())
        (beat 0.0)
        (pos start-pos))
    (dolist (char (string-to-list pattern-string))
      (cond
       ((memq char '(?x ?X ?o ?1 ?2 ?3 ?4 ?5 ?6 ?7 ?8 ?9))
        (push (cons pos beat) slots)
        (setq beat (+ beat 0.25))
        (setq pos (1+ pos)))
       ((memq char '(?. ?_ ?0))
        (push (cons pos beat) slots)
        (setq beat (+ beat 0.25))
        (setq pos (1+ pos)))
       ((eq char ?-)
        (push (cons pos beat) slots)
        (setq beat (+ beat 0.25))
        (setq pos (1+ pos)))
       ((memq char '(?| ?\s ?\n ?\r ?\t))
        (setq pos (1+ pos)))))
    (cons (nreverse slots) beat)))

(defun vibelang-viz--parse-notes-slots (notes-string start-pos)
  "Parse NOTES-STRING from START-POS. Return (slots . total-length)."
  (let ((slots '())
        (beat 0.0)
        (str-pos 0)
        (buf-pos start-pos)
        (len (length notes-string)))
    (while (< str-pos len)
      (let ((char (aref notes-string str-pos)))
        (cond
         ((memq char '(?\s ?| ?\n ?\r ?\t))
          (setq str-pos (1+ str-pos))
          (setq buf-pos (1+ buf-pos)))
         ((eq char ?\[)
          (let ((chord-start buf-pos))
            (while (and (< str-pos len)
                        (not (eq (aref notes-string str-pos) ?\])))
              (setq str-pos (1+ str-pos))
              (setq buf-pos (1+ buf-pos)))
            (when (and (< str-pos len) (eq (aref notes-string str-pos) ?\]))
              (setq str-pos (1+ str-pos))
              (setq buf-pos (1+ buf-pos)))
            (push (cons chord-start beat) slots)
            (setq beat (+ beat 1.0))))
         ((or (and (>= char ?A) (<= char ?G))
              (and (>= char ?a) (<= char ?g)))
          (let ((note-start buf-pos))
            (setq str-pos (1+ str-pos))
            (setq buf-pos (1+ buf-pos))
            (when (and (< str-pos len)
                       (memq (aref notes-string str-pos) '(?# ?b)))
              (setq str-pos (1+ str-pos))
              (setq buf-pos (1+ buf-pos)))
            (while (and (< str-pos len)
                        (let ((c (aref notes-string str-pos)))
                          (and (>= c ?0) (<= c ?9))))
              (setq str-pos (1+ str-pos))
              (setq buf-pos (1+ buf-pos)))
            (when (and (< str-pos len) (eq (aref notes-string str-pos) ?:))
              (setq str-pos (1+ str-pos))
              (setq buf-pos (1+ buf-pos))
              (while (and (< str-pos len)
                          (let ((c (aref notes-string str-pos)))
                            (or (and (>= c ?a) (<= c ?z))
                                (and (>= c ?A) (<= c ?Z))
                                (and (>= c ?0) (<= c ?9)))))
                (setq str-pos (1+ str-pos))
                (setq buf-pos (1+ buf-pos))))
            (push (cons note-start beat) slots)
            (setq beat (+ beat 1.0))))
         ((and (>= char ?1) (<= char ?9))
          (let ((degree-start buf-pos))
            (setq str-pos (1+ str-pos))
            (setq buf-pos (1+ buf-pos))
            (while (and (< str-pos len)
                        (eq (aref notes-string str-pos) ?'))
              (setq str-pos (1+ str-pos))
              (setq buf-pos (1+ buf-pos)))
            (push (cons degree-start beat) slots)
            (setq beat (+ beat 1.0))))
         ((eq char ?-)
          (push (cons buf-pos beat) slots)
          (setq beat (+ beat 1.0))
          (setq str-pos (1+ str-pos))
          (setq buf-pos (1+ buf-pos)))
         ((eq char ?.)
          (push (cons buf-pos beat) slots)
          (setq beat (+ beat 1.0))
          (setq str-pos (1+ str-pos))
          (setq buf-pos (1+ buf-pos)))
         (t
          (setq str-pos (1+ str-pos))
          (setq buf-pos (1+ buf-pos))))))
    (cons (nreverse slots) beat)))

;;; Main visualization update - called on each server tick

(defun vibelang-viz--update-all (data)
  "Update all visualizations based on playback DATA."
  (when vibelang-viz--enabled
    (dolist (buffer (buffer-list))
      (when (with-current-buffer buffer
              (and (derived-mode-p 'vibelang-mode)
                   vibelang-visualization-mode))
        (with-current-buffer buffer
          (setq vibelang-viz--last-playback-data data)
          (setq vibelang-viz--last-update-time (float-time))
          (setq vibelang-viz--transport (alist-get 'transport data))
          (vibelang-viz--update-buffer data))))))

(defun vibelang-viz--update-buffer (data)
  "Update visualization in current buffer based on DATA."
  (vibelang-viz--scan-patterns)
  ;; Clear playing overlays (cheap, line-based)
  (vibelang-viz--clear-playing-overlays)
  ;; Clear clip overlays (need full rebuild for progress bars)
  (vibelang-viz--clear-clip-overlays)
  ;; Track which beat overlays are still in use
  (let ((active-keys (make-hash-table :test 'equal))
        (patterns (alist-get 'patterns data))
        (melodies (alist-get 'melodies data))
        (sequences (alist-get 'sequences data)))
    ;; Update patterns
    (seq-do (lambda (pattern)
              (let* ((name (alist-get 'name pattern))
                     (playing (eq (alist-get 'playing pattern) t))
                     (loop-pos (or (alist-get 'loop_position pattern) 0))
                     (loop-len (or (alist-get 'loop_length pattern) 1))
                     (key (format "pattern:%s" name)))
                (if playing
                    (progn
                      (puthash key t active-keys)
                      (vibelang-viz--highlight-pattern name loop-pos loop-len))
                  (vibelang-viz--remove-beat-overlay key))))
            patterns)
    ;; Update melodies
    (seq-do (lambda (melody)
              (let* ((name (alist-get 'name melody))
                     (playing (eq (alist-get 'playing melody) t))
                     (loop-pos (or (alist-get 'loop_position melody) 0))
                     (loop-len (or (alist-get 'loop_length melody) 1))
                     (key (format "melody:%s" name)))
                (if playing
                    (progn
                      (puthash key t active-keys)
                      (vibelang-viz--highlight-melody name loop-pos loop-len))
                  (vibelang-viz--remove-beat-overlay key))))
            melodies)
    ;; Update sequences
    (seq-do (lambda (sequence)
              (let* ((name (alist-get 'name sequence))
                     (playing (eq (alist-get 'playing sequence) t))
                     (position (or (alist-get 'position sequence) 0))
                     (length (or (alist-get 'length sequence) 1))
                     (active-clips (alist-get 'active_clips sequence)))
                (when playing
                  (vibelang-viz--highlight-sequence name position length active-clips))))
            sequences)))

(defun vibelang-viz--highlight-pattern (name loop-pos loop-len)
  "Highlight current position in pattern NAME at LOOP-POS of LOOP-LEN."
  (when-let* ((info (alist-get name vibelang-viz--pattern-info nil nil #'string=)))
    (let* ((start (plist-get info :start))
           (end (plist-get info :end))
           (slots (plist-get info :slots))
           (slot-count (length slots))
           (parsed-length (plist-get info :length))
           (key (format "pattern:%s" name)))
      (vibelang-viz--add-playing-overlay start end)
      (when (and slots (> slot-count 0) (> loop-len 0) (> parsed-length 0))
        (let* ((slot-idx (vibelang-viz--loop-pos-to-slot-idx loop-pos loop-len slot-count)))
          (when (and slot-idx (>= slot-idx 0) (< slot-idx slot-count))
            (let ((slot (nth slot-idx slots)))
              (when slot
                (vibelang-viz--get-or-move-beat-overlay key (car slot) 1)))))))))

(defun vibelang-viz--highlight-melody (name loop-pos loop-len)
  "Highlight current position in melody NAME at LOOP-POS of LOOP-LEN."
  (when-let* ((info (alist-get name vibelang-viz--pattern-info nil nil #'string=)))
    (let* ((start (plist-get info :start))
           (end (plist-get info :end))
           (slots (plist-get info :slots))
           (slot-count (length slots))
           (parsed-length (plist-get info :length))
           (key (format "melody:%s" name)))
      (vibelang-viz--add-playing-overlay start end)
      (when (and slots (> slot-count 0) (> loop-len 0) (> parsed-length 0))
        (let* ((slot-idx (vibelang-viz--loop-pos-to-slot-idx-melody loop-pos loop-len slots)))
          (when (and slot-idx (>= slot-idx 0) (< slot-idx slot-count))
            (let ((slot (nth slot-idx slots)))
              (when slot
                (let ((note-end (vibelang-viz--find-note-end (car slot))))
                  (vibelang-viz--get-or-move-beat-overlay
                   key (car slot) (- note-end (car slot))))))))))))

(defun vibelang-viz--highlight-sequence (name position length active-clips)
  "Highlight playing sequence NAME at POSITION of LENGTH with ACTIVE-CLIPS."
  (when-let* ((info (alist-get name vibelang-viz--pattern-info nil nil #'string=)))
    (let* ((start (plist-get info :start))
           (end (plist-get info :end))
           (clips (plist-get info :clips)))
      (vibelang-viz--add-playing-overlay start end)
      (when active-clips
        (vibelang-viz--highlight-active-clips active-clips clips name)))))

(defun vibelang-viz--highlight-active-clips (active-clips parent-clips &optional seq-name)
  "Highlight ACTIVE-CLIPS matching PARENT-CLIPS in sequence SEQ-NAME."
  (seq-do (lambda (active-clip)
            (let* ((clip-name (alist-get 'name active-clip))
                   (clip-index (or (alist-get 'clip_index active-clip) 0))
                   (progress (or (alist-get 'progress active-clip) 0))
                   (clip-type (alist-get 'type active-clip))
                   (nested-clips (alist-get 'nested_clips active-clip)))
              (vibelang-viz--highlight-clip parent-clips clip-index progress seq-name)
              (when (and (string= clip-type "sequence") nested-clips)
                (when-let* ((nested-info (alist-get clip-name vibelang-viz--pattern-info
                                                    nil nil #'string=)))
                  (let ((nested-seq-clips (plist-get nested-info :clips))
                        (nested-start (plist-get nested-info :start))
                        (nested-end (plist-get nested-info :end)))
                    (vibelang-viz--add-playing-overlay nested-start nested-end)
                    (vibelang-viz--highlight-active-clips
                     nested-clips nested-seq-clips clip-name))))))
          active-clips))

(defun vibelang-viz--highlight-clip (clips clip-index progress &optional seq-name)
  "Highlight clip at CLIP-INDEX in CLIPS with PROGRESS."
  (dolist (clip-info clips)
    (let* ((parsed-index (nth 0 clip-info))
           (clip-start (nth 1 clip-info))
           (clip-end (nth 2 clip-info)))
      (when (= parsed-index clip-index)
        (vibelang-viz--add-clip-overlay clip-start clip-end progress clip-index seq-name)))))

(defun vibelang-viz--add-clip-overlay (start end progress &optional clip-index seq-name)
  "Add overlay for clip from START to END with PROGRESS."
  (let* ((identity-key (format "%s:%s" (or seq-name "?") (or clip-index "?")))
         (existing (seq-find (lambda (ov)
                               (and (overlay-get ov 'vibelang-clip-overlay)
                                    (equal (overlay-get ov 'vibelang-clip-identity)
                                           identity-key)))
                             (overlays-in (point-min) (point-max)))))
    (unless existing
      (let ((ov (make-overlay start end)))
        (overlay-put ov 'face 'vibelang-clip-active-face)
        (overlay-put ov 'vibelang-overlay t)
        (overlay-put ov 'vibelang-clip-overlay t)
        (overlay-put ov 'vibelang-clip-identity identity-key)
        (overlay-put ov 'priority 75)
        (let* ((bar-width 10)
               (filled (round (* progress bar-width)))
               (empty (- bar-width filled))
               (bar (concat (make-string filled ?#) (make-string empty ?-)))
               (progress-str (propertize (format " [%s] %d%%" bar (round (* progress 100)))
                                         'face 'vibelang-clip-progress-face)))
          (overlay-put ov 'after-string progress-str))))))

(defun vibelang-viz--loop-pos-to-slot-idx (loop-pos pattern-length slot-count)
  "Convert LOOP-POS to slot index given PATTERN-LENGTH and SLOT-COUNT."
  (when (and (> pattern-length 0) (> slot-count 0))
    (let* ((beats-per-slot (/ pattern-length (float slot-count)))
           (slot-idx (floor (/ loop-pos beats-per-slot))))
      (min slot-idx (1- slot-count)))))

(defun vibelang-viz--loop-pos-to-slot-idx-melody (loop-pos melody-length slots)
  "Convert LOOP-POS to slot index for melody."
  (let ((slot-count (length slots)))
    (when (and (> slot-count 0) (> melody-length 0))
      (let* ((parsed-length (if slots (cdr (car (last slots))) 1))
             (parsed-length (+ parsed-length 1.0))
             (scale (if (> parsed-length 0) (/ melody-length parsed-length) 1.0))
             (result -1)
             (idx 0))
        (dolist (slot slots)
          (let ((scaled-beat (* (cdr slot) scale)))
            (when (<= scaled-beat loop-pos)
              (setq result idx)))
          (setq idx (1+ idx)))
        result))))

(defun vibelang-viz--find-note-end (pos)
  "Find the end of a note starting at POS."
  (save-excursion
    (goto-char pos)
    (if (looking-at "[A-Ga-g][#b]?[0-9]*")
        (match-end 0)
      (1+ pos))))

;;; Clear all function for transport stop

(defun vibelang-viz--clear-all ()
  "Clear all VibeLang overlays."
  (dolist (buffer (buffer-list))
    (when (buffer-live-p buffer)
      (with-current-buffer buffer
        (vibelang-viz--clear-all-overlays)
        (setq vibelang-viz--last-playback-data nil)
        (setq vibelang-viz--transport nil)))))

(provide 'vibelang-visualization)
;;; vibelang-visualization.el ends here
