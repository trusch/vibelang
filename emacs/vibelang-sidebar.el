;;; vibelang-sidebar.el --- Project sidebar for VibeLang -*- lexical-binding: t -*-

;; Copyright (C) 2024 VibeLang Authors
;; Author: VibeLang Team
;; Keywords: multimedia, music

;;; Commentary:
;; Provides a project sidebar for VibeLang showing:
;; - Project file tree
;; - Groups with hierarchy and live meters
;; - Voices, patterns, melodies nested under groups
;; - Playing indicators and progress bars
;; - Click-to-navigate functionality

;;; Code:

(require 'vibelang-websocket)

(defgroup vibelang-sidebar nil
  "VibeLang project sidebar."
  :group 'vibelang
  :prefix "vibelang-sidebar-")

(defcustom vibelang-sidebar-width 40
  "Width of the sidebar window."
  :type 'integer
  :group 'vibelang-sidebar)

(defcustom vibelang-sidebar-position 'left
  "Position of the sidebar window."
  :type '(choice (const left) (const right))
  :group 'vibelang-sidebar)

(defcustom vibelang-sidebar-meter-width 10
  "Width of the level meter bar."
  :type 'integer
  :group 'vibelang-sidebar)

(defcustom vibelang-sidebar-show-meters t
  "Whether to show live audio meters."
  :type 'boolean
  :group 'vibelang-sidebar)

;;; Faces

(defface vibelang-sidebar-group-face
  '((((class color) (background dark))
     :foreground "#61afef" :weight bold)
    (((class color) (background light))
     :foreground "#4078f2" :weight bold))
  "Face for group names in sidebar."
  :group 'vibelang-sidebar)

(defface vibelang-sidebar-voice-face
  '((((class color) (background dark))
     :foreground "#c678dd")
    (((class color) (background light))
     :foreground "#a626a4"))
  "Face for voice names in sidebar."
  :group 'vibelang-sidebar)

(defface vibelang-sidebar-pattern-face
  '((((class color) (background dark))
     :foreground "#e5c07b")
    (((class color) (background light))
     :foreground "#c18401"))
  "Face for pattern names in sidebar."
  :group 'vibelang-sidebar)

(defface vibelang-sidebar-melody-face
  '((((class color) (background dark))
     :foreground "#98c379")
    (((class color) (background light))
     :foreground "#50a14f"))
  "Face for melody names in sidebar."
  :group 'vibelang-sidebar)

(defface vibelang-sidebar-sequence-face
  '((((class color) (background dark))
     :foreground "#56b6c2")
    (((class color) (background light))
     :foreground "#0184bc"))
  "Face for sequence names in sidebar."
  :group 'vibelang-sidebar)

(defface vibelang-sidebar-playing-face
  '((((class color) (background dark))
     :foreground "#00ff00" :weight bold)
    (((class color) (background light))
     :foreground "#00aa00" :weight bold))
  "Face for playing indicator."
  :group 'vibelang-sidebar)

(defface vibelang-sidebar-muted-face
  '((t :foreground "#888888" :strike-through t))
  "Face for muted entities."
  :group 'vibelang-sidebar)

(defface vibelang-sidebar-meter-low-face
  '((((class color) (background dark))
     :foreground "#98c379" :background "#2c3e50")
    (((class color) (background light))
     :foreground "#50a14f" :background "#ddd"))
  "Face for low meter levels."
  :group 'vibelang-sidebar)

(defface vibelang-sidebar-meter-mid-face
  '((((class color) (background dark))
     :foreground "#e5c07b" :background "#2c3e50")
    (((class color) (background light))
     :foreground "#c18401" :background "#ddd"))
  "Face for medium meter levels."
  :group 'vibelang-sidebar)

(defface vibelang-sidebar-meter-high-face
  '((((class color) (background dark))
     :foreground "#e06c75" :background "#2c3e50")
    (((class color) (background light))
     :foreground "#e45649" :background "#ddd"))
  "Face for high meter levels."
  :group 'vibelang-sidebar)

(defface vibelang-sidebar-header-face
  '((t :height 1.1 :weight bold :underline t))
  "Face for section headers in sidebar."
  :group 'vibelang-sidebar)

(defface vibelang-sidebar-progress-filled-face
  '((((class color) (background dark))
     :foreground "#98c379")
    (((class color) (background light))
     :foreground "#50a14f"))
  "Face for filled portion of progress bar."
  :group 'vibelang-sidebar)

(defface vibelang-sidebar-progress-empty-face
  '((((class color) (background dark))
     :foreground "#3e4451")
    (((class color) (background light))
     :foreground "#d0d0d0"))
  "Face for empty portion of progress bar."
  :group 'vibelang-sidebar)

(defface vibelang-sidebar-progress-head-face
  '((((class color) (background dark))
     :foreground "#61afef" :weight bold)
    (((class color) (background light))
     :foreground "#4078f2" :weight bold))
  "Face for the playhead indicator in progress bar."
  :group 'vibelang-sidebar)

(defface vibelang-sidebar-progress-percent-face
  '((((class color) (background dark))
     :foreground "#5c6370")
    (((class color) (background light))
     :foreground "#9ca0a4"))
  "Face for percentage text in progress bar."
  :group 'vibelang-sidebar)

(defface vibelang-sidebar-fade-face
  '((((class color) (background dark))
     :foreground "#ff9966")
    (((class color) (background light))
     :foreground "#d97706"))
  "Face for faded parameter names in sidebar."
  :group 'vibelang-sidebar)

(defface vibelang-sidebar-fade-value-face
  '((((class color) (background dark))
     :foreground "#fbbf24" :weight bold)
    (((class color) (background light))
     :foreground "#b45309" :weight bold))
  "Face for current fade values in sidebar."
  :group 'vibelang-sidebar)

;;; Buffer and state

(defvar vibelang-sidebar--buffer nil
  "The sidebar buffer.")

(defvar vibelang-sidebar--window nil
  "The sidebar window.")

(defvar vibelang-sidebar--project-state nil
  "Current project state for sidebar display.")

(defvar vibelang-sidebar--expanded-groups nil
  "List of group names that are expanded in the tree view.")

;;; Mode definition

(defvar vibelang-sidebar-mode-map
  (let ((map (make-sparse-keymap)))
    (define-key map (kbd "RET") #'vibelang-sidebar-action)
    (define-key map (kbd "TAB") #'vibelang-sidebar-toggle-expand)
    (define-key map (kbd "g") #'vibelang-sidebar-refresh)
    (define-key map (kbd "q") #'vibelang-sidebar-quit)
    (define-key map (kbd "m") #'vibelang-sidebar-toggle-mute)
    (define-key map (kbd "s") #'vibelang-sidebar-toggle-solo)
    (define-key map (kbd "p") #'vibelang-sidebar-play-stop)
    map)
  "Keymap for `vibelang-sidebar-mode'.")

(define-derived-mode vibelang-sidebar-mode special-mode "VibeLang-Sidebar"
  "Major mode for VibeLang project sidebar."
  :group 'vibelang-sidebar
  (setq-local truncate-lines t)
  (setq-local buffer-read-only t)
  (setq-local cursor-type nil)
  (setq-local show-trailing-whitespace nil)
  (hl-line-mode 1))

;;; Public commands

;;;###autoload
(defun vibelang-sidebar-open ()
  "Open the VibeLang project sidebar."
  (interactive)
  (unless (vibelang-sidebar--buffer-live-p)
    (vibelang-sidebar--create-buffer))
  (vibelang-sidebar--display-buffer)
  (add-hook 'vibelang--playback-state-hook #'vibelang-sidebar--on-playback-update)
  (vibelang-sidebar-refresh))

(defun vibelang-sidebar-close ()
  "Close the VibeLang project sidebar."
  (interactive)
  (remove-hook 'vibelang--playback-state-hook #'vibelang-sidebar--on-playback-update)
  (when (vibelang-sidebar--buffer-live-p)
    (when (window-live-p vibelang-sidebar--window)
      (delete-window vibelang-sidebar--window))
    (kill-buffer vibelang-sidebar--buffer))
  (setq vibelang-sidebar--buffer nil
        vibelang-sidebar--window nil))

(defun vibelang-sidebar-toggle ()
  "Toggle the VibeLang project sidebar."
  (interactive)
  (if (vibelang-sidebar--buffer-live-p)
      (vibelang-sidebar-close)
    (vibelang-sidebar-open)))

(defun vibelang-sidebar-quit ()
  "Close the sidebar."
  (interactive)
  (vibelang-sidebar-close))

(defun vibelang-sidebar-refresh ()
  "Refresh the sidebar display."
  (interactive)
  (when (vibelang-sidebar--buffer-live-p)
    (vibelang-sidebar--render)))

;;; Tree navigation

(defun vibelang-sidebar-toggle-expand ()
  "Toggle expansion of the current tree node."
  (interactive)
  (when-let ((node (get-text-property (point) 'vibelang-node)))
    (let ((name (plist-get node :name))
          (type (plist-get node :type)))
      (when (eq type 'group)
        (if (member name vibelang-sidebar--expanded-groups)
            (setq vibelang-sidebar--expanded-groups
                  (delete name vibelang-sidebar--expanded-groups))
          (push name vibelang-sidebar--expanded-groups))
        (vibelang-sidebar-refresh)))))

(defun vibelang-sidebar-action ()
  "Perform default action on the current node."
  (interactive)
  (when-let ((node (get-text-property (point) 'vibelang-node)))
    (let ((file (plist-get node :file))
          (line (plist-get node :line)))
      (when (and file line)
        (vibelang-sidebar--goto-definition file line)))))

(defun vibelang-sidebar-toggle-mute ()
  "Toggle mute on the current entity."
  (interactive)
  (when-let ((node (get-text-property (point) 'vibelang-node)))
    (message "TODO: Send mute command for %s" (plist-get node :name))))

(defun vibelang-sidebar-toggle-solo ()
  "Toggle solo on the current entity."
  (interactive)
  (when-let ((node (get-text-property (point) 'vibelang-node)))
    (message "TODO: Send solo command for %s" (plist-get node :name))))

(defun vibelang-sidebar-play-stop ()
  "Toggle play/stop on the current entity."
  (interactive)
  (when-let ((node (get-text-property (point) 'vibelang-node)))
    (let ((type (plist-get node :type))
          (name (plist-get node :name))
          (playing (plist-get node :playing)))
      (cond
       ((memq type '(pattern melody))
        (message "TODO: Send %s command for %s %s"
                 (if playing "stop" "start") type name))
       ((eq type 'sequence)
        (message "TODO: Send %s command for sequence %s"
                 (if playing "stop" "start") name))))))

;;; Internal functions

(defun vibelang-sidebar--buffer-live-p ()
  "Return non-nil if the sidebar buffer is live."
  (and vibelang-sidebar--buffer (buffer-live-p vibelang-sidebar--buffer)))

(defun vibelang-sidebar--create-buffer ()
  "Create the sidebar buffer."
  (setq vibelang-sidebar--buffer (get-buffer-create "*VibeLang Project*"))
  (with-current-buffer vibelang-sidebar--buffer
    (vibelang-sidebar-mode)))

(defun vibelang-sidebar--display-buffer ()
  "Display the sidebar buffer in a side window."
  (let ((window (display-buffer-in-side-window
                 vibelang-sidebar--buffer
                 `((side . ,vibelang-sidebar-position)
                   (window-width . ,vibelang-sidebar-width)
                   (slot . 0)
                   (window-parameters . ((no-delete-other-windows . t)))))))
    (setq vibelang-sidebar--window window)
    (set-window-dedicated-p window t)))

(defun vibelang-sidebar--goto-definition (file line)
  "Navigate to FILE at LINE in another window."
  (let ((win (if (eq vibelang-sidebar-position 'left)
                 (window-in-direction 'right vibelang-sidebar--window)
               (window-in-direction 'left vibelang-sidebar--window))))
    (unless win
      (setq win (split-window vibelang-sidebar--window nil 'right)))
    (select-window win)
    (find-file file)
    (goto-char (point-min))
    (forward-line (1- line))))

(defun vibelang-sidebar--on-playback-update (data)
  "Handle playback state update with DATA."
  (setq vibelang-sidebar--project-state data)
  (when (vibelang-sidebar--buffer-live-p)
    (vibelang-sidebar--render)))

;;; Rendering

(defun vibelang-sidebar--render ()
  "Render the sidebar content."
  (when (vibelang-sidebar--buffer-live-p)
    (let ((inhibit-read-only t)
          (pos (point)))
      (with-current-buffer vibelang-sidebar--buffer
        (erase-buffer)
        (vibelang-sidebar--render-header)
        (vibelang-sidebar--render-transport)
        (vibelang-sidebar--render-groups)
        (vibelang-sidebar--render-sequences)
        (vibelang-sidebar--render-fades)
        (goto-char (min pos (point-max)))))))

(defun vibelang-sidebar--render-header ()
  "Render the sidebar header."
  (insert (propertize "VIBELANG PROJECT" 'face 'vibelang-sidebar-header-face))
  (insert "\n")
  (if (vibelang-ws-connected-p)
      (insert (propertize " Connected" 'face 'success))
    (insert (propertize " Disconnected" 'face 'error)))
  (insert "\n\n"))

(defun vibelang-sidebar--render-transport ()
  "Render transport state."
  (when-let ((state vibelang-sidebar--project-state))
    (let* ((transport (alist-get 'transport state))
           (playing (eq (alist-get 'playing transport) t))
           (bar (alist-get 'bar transport))
           (beat-in-bar (alist-get 'beat_in_bar transport))
           (bpm (alist-get 'bpm transport)))
      (insert (propertize "TRANSPORT" 'face 'vibelang-sidebar-header-face))
      (insert "\n")
      (insert (format " %s Bar %d.%d @ %.0f BPM\n\n"
                      (if playing
                          (propertize ">" 'face 'vibelang-sidebar-playing-face)
                        (propertize "||" 'face 'font-lock-comment-face))
                      (1+ (or bar 0))
                      (1+ (or beat-in-bar 0))
                      (or bpm 120))))))

(defun vibelang-sidebar--render-groups ()
  "Render groups tree."
  (insert (propertize "GROUPS" 'face 'vibelang-sidebar-header-face))
  (insert "\n")
  (when-let ((state vibelang-sidebar--project-state))
    (let ((groups (alist-get 'groups state)))
      (if groups
          (seq-do (lambda (group)
                    (when (null (alist-get 'parent group))
                      (vibelang-sidebar--render-group group state 0)))
                  groups)
        (insert "  (no groups)\n"))))
  (insert "\n"))

(defun vibelang-sidebar--render-group (group state indent)
  "Render GROUP at INDENT level with STATE context."
  (let* ((name (alist-get 'name group))
         (muted (eq (alist-get 'muted group) t))
         (meter-peak (alist-get 'meter_peak group))
         (voices (alist-get 'voices group))
         (patterns (alist-get 'patterns group))
         (melodies (alist-get 'melodies group))
         (expanded (member name vibelang-sidebar--expanded-groups))
         (prefix (make-string (* indent 2) ?\s))
         (icon (if expanded "" "")))
    ;; Group line with meter
    (insert prefix)
    (insert icon " ")
    (insert (propertize name 'face (if muted
                                        'vibelang-sidebar-muted-face
                                      'vibelang-sidebar-group-face)))
    (when (and vibelang-sidebar-show-meters meter-peak)
      (insert " ")
      (insert (vibelang-sidebar--format-meter meter-peak)))
    (insert "\n")
    ;; Add text properties for this line
    (put-text-property (line-beginning-position 0) (1- (point))
                       'vibelang-node
                       (list :type 'group :name name :muted muted))
    ;; Render children if expanded
    (when expanded
      ;; Voices
      (dolist (voice-name (append voices nil))
        (vibelang-sidebar--render-voice voice-name state (1+ indent)))
      ;; Patterns
      (dolist (pattern-name (append patterns nil))
        (vibelang-sidebar--render-pattern pattern-name state (1+ indent)))
      ;; Melodies
      (dolist (melody-name (append melodies nil))
        (vibelang-sidebar--render-melody melody-name state (1+ indent))))))

(defun vibelang-sidebar--render-voice (name state indent)
  "Render voice NAME at INDENT level with STATE context."
  (let* ((voice-data (seq-find (lambda (v) (string= (alist-get 'name v) name))
                               (alist-get 'voices state)))
         (synth (or (alist-get 'synth voice-data) "?"))
         (muted (eq (alist-get 'muted voice-data) t))
         (prefix (make-string (* indent 2) ?\s)))
    (insert prefix)
    (insert (propertize (format " %s" name)
                        'face (if muted
                                   'vibelang-sidebar-muted-face
                                 'vibelang-sidebar-voice-face)))
    (insert (propertize (format " [%s]" synth) 'face 'font-lock-comment-face))
    (insert "\n")
    (put-text-property (line-beginning-position 0) (1- (point))
                       'vibelang-node
                       (list :type 'voice :name name :muted muted))))

(defun vibelang-sidebar--render-pattern (name state indent)
  "Render pattern NAME at INDENT level with STATE context."
  (let* ((pattern-data (seq-find (lambda (p) (string= (alist-get 'name p) name))
                                 (alist-get 'patterns state)))
         (playing (eq (alist-get 'playing pattern-data) t))
         (loop-pos (alist-get 'loop_position pattern-data))
         (loop-len (alist-get 'loop_length pattern-data))
         (prefix (make-string (* indent 2) ?\s))
         (icon (if playing
                   (propertize "" 'face 'vibelang-sidebar-playing-face)
                 "")))
    (insert prefix)
    (insert icon " ")
    (insert (propertize name 'face 'vibelang-sidebar-pattern-face))
    (when (and playing loop-pos loop-len (> loop-len 0))
      (insert " ")
      (insert (vibelang-sidebar--format-progress loop-pos loop-len)))
    (insert "\n")
    (put-text-property (line-beginning-position 0) (1- (point))
                       'vibelang-node
                       (list :type 'pattern :name name :playing playing))))

(defun vibelang-sidebar--render-melody (name state indent)
  "Render melody NAME at INDENT level with STATE context."
  (let* ((melody-data (seq-find (lambda (m) (string= (alist-get 'name m) name))
                                (alist-get 'melodies state)))
         (playing (eq (alist-get 'playing melody-data) t))
         (loop-pos (alist-get 'loop_position melody-data))
         (loop-len (alist-get 'loop_length melody-data))
         (prefix (make-string (* indent 2) ?\s))
         (icon (if playing
                   (propertize "" 'face 'vibelang-sidebar-playing-face)
                 "")))
    (insert prefix)
    (insert icon " ")
    (insert (propertize name 'face 'vibelang-sidebar-melody-face))
    (when (and playing loop-pos loop-len (> loop-len 0))
      (insert " ")
      (insert (vibelang-sidebar--format-progress loop-pos loop-len)))
    (insert "\n")
    (put-text-property (line-beginning-position 0) (1- (point))
                       'vibelang-node
                       (list :type 'melody :name name :playing playing))))

(defun vibelang-sidebar--render-sequences ()
  "Render sequences section."
  (insert (propertize "SEQUENCES" 'face 'vibelang-sidebar-header-face))
  (insert "\n")
  (when-let ((state vibelang-sidebar--project-state))
    (let ((sequences (alist-get 'sequences state)))
      (if sequences
          (seq-do #'vibelang-sidebar--render-sequence sequences)
        (insert "  (no sequences)\n"))))
  (insert "\n"))

(defun vibelang-sidebar--render-sequence (seq)
  "Render sequence SEQ on multiple lines for readability."
  (let* ((name (alist-get 'name seq))
         (playing (eq (alist-get 'playing seq) t))
         (pos (alist-get 'position seq))
         (len (alist-get 'length seq))
         (looping (eq (alist-get 'looping seq) t))
         (icon (if playing
                   (propertize "" 'face 'vibelang-sidebar-playing-face)
                 "")))
    ;; Line 1: name
    (insert " " icon " ")
    (insert (propertize name 'face 'vibelang-sidebar-sequence-face))
    (when looping (insert " "))
    (insert "\n")
    (put-text-property (line-beginning-position 0) (1- (point))
                       'vibelang-node
                       (list :type 'sequence :name name :playing playing))
    ;; Line 2: progress bar and position
    (when (and pos len (> len 0))
      (insert "    ")
      (insert (vibelang-sidebar--format-progress pos len))
      (insert "\n"))))

(defun vibelang-sidebar--render-fades ()
  "Render active fades section showing current parameter values."
  (condition-case err
      (when-let ((state vibelang-sidebar--project-state))
        (let ((fades (alist-get 'fades state)))
          (when fades
            (insert (propertize "FADED PARAMS" 'face 'vibelang-sidebar-header-face))
            (insert "\n")
            (seq-do #'vibelang-sidebar--render-fade fades)
            (insert "\n"))))
    (error
     (insert (format "  (error: %s)\n" (error-message-string err))))))

(defun vibelang-sidebar--render-fade (fade)
  "Render a single FADE entry on multiple lines for readability."
  (let* ((target-name (or (alist-get 'target_name fade) "?"))
         (param (or (alist-get 'param fade) "?"))
         (current-value (alist-get 'current_value fade))
         (target-value (alist-get 'target_value fade))
         (start-value (alist-get 'start_value fade))
         (progress (alist-get 'progress fade))
         ;; Determine direction: up if target > start, down otherwise
         (going-up (and (numberp target-value) (numberp start-value)
                        (> target-value start-value)))
         (arrow (if going-up " ⤴ " " ⤵ "))
         ;; Format value: show 2 decimal places, or fewer if it's a round number
         (value-str (vibelang-sidebar--format-value current-value))
         (target-str (vibelang-sidebar--format-value target-value)))
    ;; Line 1: target.param with direction arrow
    (insert arrow)
    (insert (propertize target-name 'face 'vibelang-sidebar-fade-face))
    (insert ".")
    (insert (propertize param 'face 'vibelang-sidebar-fade-face))
    (insert "\n")
    ;; Line 2: current value → target value + progress
    (insert "    ")
    (insert (propertize value-str 'face 'vibelang-sidebar-fade-value-face))
    (insert (propertize (format " → %s" target-str) 'face 'font-lock-comment-face))
    (when (and progress (numberp progress) (< progress 1.0))
      (insert " ")
      (insert (vibelang-sidebar--format-fade-progress progress)))
    (insert "\n")))

(defun vibelang-sidebar--format-value (value)
  "Format VALUE for display, removing unnecessary decimal places."
  (if (and value (numberp value))
      (let ((frac (abs (- value (truncate value)))))
        (cond
         ((< frac 0.01) (format "%.0f" value))
         ((< (abs (- (* frac 10) (truncate (* frac 10)))) 0.01) (format "%.1f" value))
         (t (format "%.2f" value))))
    "?"))

(defun vibelang-sidebar--format-fade-progress (progress)
  "Format fade PROGRESS (0.0 to 1.0) as a small progress indicator."
  (let* ((width 6)
         (filled (floor (* width progress)))
         (filled (max 0 (min filled width))))
    (concat
     (propertize (make-string filled ?▰) 'face 'vibelang-sidebar-progress-filled-face)
     (propertize (make-string (- width filled) ?▱) 'face 'vibelang-sidebar-progress-empty-face))))

;;; Formatting helpers

(defun vibelang-sidebar--format-meter (peak)
  "Format PEAK level as ASCII meter bar."
  (let* ((width vibelang-sidebar-meter-width)
         ;; Convert 0-1 linear to dB, then to visual position
         (db (if (> peak 0.0001) (* 20 (log peak 10)) -60))
         ;; Map -60dB to 0dB to 0-width
         (filled (max 0 (min width (round (* width (/ (+ db 60) 60))))))
         (bar-str (concat (make-string filled ?=) (make-string (- width filled) ?-))))
    (concat "["
            (propertize (substring bar-str 0 (min filled (/ width 2)))
                        'face 'vibelang-sidebar-meter-low-face)
            (propertize (substring bar-str (min filled (/ width 2))
                                   (min filled (* 3 (/ width 4))))
                        'face 'vibelang-sidebar-meter-mid-face)
            (propertize (substring bar-str (min filled (* 3 (/ width 4))))
                        'face 'vibelang-sidebar-meter-high-face)
            "]")))

(defun vibelang-sidebar--format-progress (pos length)
  "Format progress bar for POS out of LENGTH beats.
Returns a visually appealing progress bar with percentage."
  (let* ((width 10)
         (percent (if (> length 0)
                      (* 100 (/ (float pos) length))
                    0))
         (filled (if (> length 0)
                     (floor (* width (/ (float pos) length)))
                   0))
         (filled (min filled (1- width)))  ; Leave room for playhead
         (empty (- width filled 1))        ; -1 for playhead
         ;; Build the bar parts
         (filled-str (if (> filled 0)
                         (propertize (make-string filled ?━)
                                     'face 'vibelang-sidebar-progress-filled-face)
                       ""))
         (head-str (propertize "●" 'face 'vibelang-sidebar-progress-head-face))
         (empty-str (if (> empty 0)
                        (propertize (make-string empty ?─)
                                    'face 'vibelang-sidebar-progress-empty-face)
                      ""))
         ;; Format percentage
         (percent-str (propertize (format " %3.0f%%" percent)
                                  'face 'vibelang-sidebar-progress-percent-face)))
    (concat "╶" filled-str head-str empty-str "╴" percent-str)))

(provide 'vibelang-sidebar)
;;; vibelang-sidebar.el ends here
