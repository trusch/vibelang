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
;; - Focused inspector details for the selected or current entity

;;; Code:

(require 'seq)
(require 'subr-x)
(require 'project)
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

(defcustom vibelang-sidebar-auto-inspect-point t
  "Whether the inspector should follow entity names at point in VibeLang buffers."
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

(defface vibelang-sidebar-dim-face
  '((((class color) (background dark))
     :foreground "#7f848e")
    (((class color) (background light))
     :foreground "#8c8c8c"))
  "Face for secondary sidebar text."
  :group 'vibelang-sidebar)

(defface vibelang-sidebar-inspector-key-face
  '((((class color) (background dark))
     :foreground "#abb2bf" :weight bold)
    (((class color) (background light))
     :foreground "#383a42" :weight bold))
  "Face for inspector keys."
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

(defvar vibelang-sidebar--inspector-node nil
  "Explicitly focused node for the inspector.")

(defvar vibelang-sidebar--last-point-signature nil
  "Last point signature used to avoid redundant inspector refreshes.")

;;; Mode definition

(defvar vibelang-sidebar-mode-map
  (let ((map (make-sparse-keymap)))
    (define-key map (kbd "RET") #'vibelang-sidebar-action)
    (define-key map (kbd "TAB") #'vibelang-sidebar-toggle-expand)
    (define-key map (kbd "g") #'vibelang-sidebar-refresh)
    (define-key map (kbd "i") #'vibelang-sidebar-focus-current-entity)
    (define-key map (kbd "r") #'vibelang-sidebar-request-resync)
    (define-key map (kbd "q") #'vibelang-sidebar-quit)
    (define-key map (kbd "m") #'vibelang-sidebar-toggle-mute)
    (define-key map (kbd "s") #'vibelang-sidebar-toggle-solo)
    (define-key map (kbd "p") #'vibelang-sidebar-play-stop)
    (define-key map (kbd "SPC") #'vibelang-sidebar-transport-toggle)
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
  (add-hook 'vibelang--connection-state-hook #'vibelang-sidebar--on-connection-state-change)
  (add-hook 'vibelang--command-result-hook #'vibelang-sidebar--on-command-result)
  (add-hook 'post-command-hook #'vibelang-sidebar--track-point)
  (vibelang-sidebar-refresh))

(defun vibelang-sidebar-close ()
  "Close the VibeLang project sidebar."
  (interactive)
  (remove-hook 'vibelang--playback-state-hook #'vibelang-sidebar--on-playback-update)
  (remove-hook 'vibelang--connection-state-hook #'vibelang-sidebar--on-connection-state-change)
  (remove-hook 'vibelang--command-result-hook #'vibelang-sidebar--on-command-result)
  (remove-hook 'post-command-hook #'vibelang-sidebar--track-point)
  (when (vibelang-sidebar--buffer-live-p)
    (when (window-live-p vibelang-sidebar--window)
      (delete-window vibelang-sidebar--window))
    (kill-buffer vibelang-sidebar--buffer))
  (setq vibelang-sidebar--buffer nil
        vibelang-sidebar--window nil
        vibelang-sidebar--inspector-node nil
        vibelang-sidebar--last-point-signature nil))

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

(defun vibelang-sidebar-focus-current-entity ()
  "Focus the inspector on the entity at point in the current VibeLang buffer."
  (interactive)
  (let ((node (vibelang-sidebar--entity-at-point)))
    (if node
        (progn
          (setq vibelang-sidebar--inspector-node node)
          (vibelang-sidebar-refresh)
          (message "VibeLang inspector focused on %s %s"
                   (plist-get node :type)
                   (plist-get node :name)))
      (message "No VibeLang entity found at point"))))

(defun vibelang-sidebar-request-resync ()
  "Request a fresh playback snapshot from the runtime."
  (interactive)
  (vibelang-ws-request-resync "manual sidebar refresh")
  (vibelang-sidebar-refresh))

(defun vibelang-sidebar-transport-toggle ()
  "Toggle transport start/stop from the sidebar."
  (interactive)
  (let* ((transport (alist-get 'transport vibelang-sidebar--project-state))
         (playing (eq (alist-get 'playing transport) t)))
    (vibelang-ws-send-command (if playing "transport.stop" "transport.start"))
    (vibelang-sidebar-refresh)))

;;; Tree navigation

(defun vibelang-sidebar-toggle-expand ()
  "Toggle expansion of the current tree node."
  (interactive)
  (when-let ((node (get-text-property (point) 'vibelang-node)))
    (setq vibelang-sidebar--inspector-node node)
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
    (setq vibelang-sidebar--inspector-node node)
    (let ((type (plist-get node :type)))
      (cond
       ((eq type 'group)
        (vibelang-sidebar-toggle-expand))
       (t
        (unless (vibelang-sidebar--goto-node-definition node)
          (message "No source location found for %s %s"
                   type (plist-get node :name)))))
      (vibelang-sidebar-refresh))))

(defun vibelang-sidebar-toggle-mute ()
  "Toggle mute on the current entity."
  (interactive)
  (when-let ((node (or (get-text-property (point) 'vibelang-node)
                       (vibelang-sidebar--inspector-target))))
    (setq vibelang-sidebar--inspector-node node)
    (pcase (plist-get node :type)
      ('group
       (vibelang-sidebar--entity-post
        'group
        (plist-get node :name)
        (if (plist-get node :muted) "unmute" "mute")))
      ('voice
       (vibelang-sidebar--entity-post
        'voice
        (plist-get node :name)
        (if (plist-get node :muted) "unmute" "mute")))
      (_
       (user-error "Mute is only available for groups and voices")))
    (vibelang-sidebar-refresh)))

(defun vibelang-sidebar-toggle-solo ()
  "Toggle solo on the current entity."
  (interactive)
  (when-let ((node (or (get-text-property (point) 'vibelang-node)
                       (vibelang-sidebar--inspector-target))))
    (setq vibelang-sidebar--inspector-node node)
    (pcase (plist-get node :type)
      ('group
       (vibelang-sidebar--entity-post
        'group
        (plist-get node :name)
        (if (plist-get node :soloed) "unsolo" "solo")))
      (_
       (user-error "Solo is only available for groups")))
    (vibelang-sidebar-refresh)))

(defun vibelang-sidebar-play-stop ()
  "Toggle play/stop on the current entity."
  (interactive)
  (when-let ((node (or (get-text-property (point) 'vibelang-node)
                       (vibelang-sidebar--inspector-target))))
    (setq vibelang-sidebar--inspector-node node)
    (let ((type (plist-get node :type))
          (name (plist-get node :name))
          (playing (plist-get node :playing)))
      (pcase type
        ('pattern
         (vibelang-sidebar--entity-post 'pattern name (if playing "stop" "start") '(())))
        ('melody
         (vibelang-sidebar--entity-post 'melody name (if playing "stop" "start") '(())))
        ('sequence
         (vibelang-sidebar--entity-post 'sequence name (if playing "stop" "start")
                                        (unless playing '((play_once . :json-false)))))
        ('voice
         (vibelang-sidebar--entity-post 'voice name (if (plist-get node :active) "stop" "trigger")
                                        (when (not (plist-get node :active)) '(()))))
        (_
         (user-error "No play/stop action for %s" type))))
    (vibelang-sidebar-refresh)))

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

(defun vibelang-sidebar--goto-node-definition (node)
  "Jump to the best known source location for NODE.
Return non-nil when a location was found."
  (when-let ((location (vibelang-sidebar--find-node-location node)))
    (vibelang-sidebar--goto-definition (car location) (cdr location))
    t))

(defun vibelang-sidebar--on-playback-update (data)
  "Handle playback state update with DATA."
  (setq vibelang-sidebar--project-state data)
  (when (vibelang-sidebar--buffer-live-p)
    (vibelang-sidebar--render)))

(defun vibelang-sidebar--on-connection-state-change (_state _detail)
  "Refresh the sidebar when the WebSocket connection state changes."
  (when (vibelang-sidebar--buffer-live-p)
    (vibelang-sidebar--render)))

(defun vibelang-sidebar--on-command-result (_result)
  "Refresh the sidebar when a VibeLang command finishes."
  (when (vibelang-sidebar--buffer-live-p)
    (vibelang-sidebar--render)))

(defun vibelang-sidebar--track-point ()
  "Refresh inspector context when point moves in relevant buffers."
  (when (and (vibelang-sidebar--buffer-live-p)
             vibelang-sidebar-auto-inspect-point)
    (let ((signature (cond
                      ((eq (current-buffer) vibelang-sidebar--buffer)
                       (list 'sidebar (point) (get-text-property (point) 'vibelang-node)))
                      ((derived-mode-p 'vibelang-mode)
                       (list 'mode (current-buffer) (point)))
                      (t nil))))
      (unless (equal signature vibelang-sidebar--last-point-signature)
        (setq vibelang-sidebar--last-point-signature signature)
        (when signature
          (vibelang-sidebar-refresh))))))

;;; Rendering

(defun vibelang-sidebar--render ()
  "Render the sidebar content."
  (when (vibelang-sidebar--buffer-live-p)
    (let ((inhibit-read-only t)
          (sidebar-pos (and (eq (current-buffer) vibelang-sidebar--buffer) (point)))
          (focus-node (when (eq (current-buffer) vibelang-sidebar--buffer)
                        (get-text-property (point) 'vibelang-node))))
      (with-current-buffer vibelang-sidebar--buffer
        (erase-buffer)
        (vibelang-sidebar--render-header)
        (vibelang-sidebar--render-transport)
        (vibelang-sidebar--render-groups)
        (vibelang-sidebar--render-sequences)
        (vibelang-sidebar--render-fades)
        (vibelang-sidebar--render-inspector)
        (goto-char (point-min))
        (when sidebar-pos
          (goto-char (min sidebar-pos (point-max))))
        (when focus-node
          (setq vibelang-sidebar--inspector-node focus-node))))))

(defun vibelang-sidebar--render-header ()
  "Render the sidebar header."
  (let* ((ws-state (vibelang-ws-connection-state))
         (status (vibelang-ws-status-string))
         (face (cond
                ((eq ws-state 'synced) 'success)
                ((eq ws-state 'stale) 'warning)
                ((eq ws-state 'protocol-mismatch) 'error)
                ((eq ws-state 'disconnected) 'error)
                (t 'font-lock-comment-face)))
         (host (or (vibelang-ws-host) "127.0.0.1"))
         (port (or (vibelang-ws-port) 1606))
         (command (vibelang-ws-last-command-result))
         (command-face (if (plist-get command :success) 'success 'error)))
    (insert (propertize "VIBELANG PROJECT" 'face 'vibelang-sidebar-header-face))
    (insert "\n")
    (insert (propertize (format " %s" status) 'face face))
    (insert "\n")
    (insert (propertize (format " %s:%s  api:%s" host port (vibelang-ws-api-port))
                        'face 'vibelang-sidebar-dim-face))
    (insert "\n")
    (when command
      (insert (propertize (format " %s" (plist-get command :message))
                          'face command-face))
      (insert "\n"))
    (insert "\n")))

(defun vibelang-sidebar--render-transport ()
  "Render transport and connection state."
  (insert (propertize "TRANSPORT" 'face 'vibelang-sidebar-header-face))
  (insert "\n")
  (let* ((state vibelang-sidebar--project-state)
         (transport (and state (alist-get 'transport state)))
         (playing (eq (alist-get 'playing transport) t))
         (bar (alist-get 'bar transport))
         (beat-in-bar (alist-get 'beat_in_bar transport))
         (beat (alist-get 'beat transport))
         (bpm (alist-get 'bpm transport))
         (time-sig (alist-get 'time_sig transport))
         (time-sig-parts (vibelang-ws-time-signature-components time-sig))
         (sig-num (car time-sig-parts))
         (sig-denom (cadr time-sig-parts))
         (last-message (vibelang-ws-last-message-time))
         (last-update (vibelang-ws-last-playback-update-time))
         (state-since (vibelang-ws-state-since)))
    (if transport
        (progn
          (insert (format " %s  bar %d.%d  @ %.0f bpm\n"
                          (if playing
                              (propertize "▶ playing" 'face 'vibelang-sidebar-playing-face)
                            (propertize "■ stopped" 'face 'font-lock-comment-face))
                          (1+ (or bar 0))
                          (1+ (or beat-in-bar 0))
                          (or bpm 120.0)))
          (insert (format " %s  beat %.2f  snapshot %s\n"
                          (propertize (format "%d/%d" sig-num sig-denom)
                                      'face 'vibelang-sidebar-dim-face)
                          (or beat 0.0)
                          (vibelang-sidebar--format-age last-update)))
          (insert (format " ws msg %s  state %s\n"
                          (vibelang-sidebar--format-age last-message)
                          (vibelang-sidebar--format-age state-since))))
      (insert (format " %s\n"
                      (propertize "Waiting for playback snapshot" 'face 'vibelang-sidebar-dim-face)))
      (insert (format " ws msg %s  state %s\n"
                      (vibelang-sidebar--format-age last-message)
                      (vibelang-sidebar--format-age state-since))))
    (when-let ((last-error (vibelang-ws-last-error)))
      (insert (propertize (format " %s\n" last-error) 'face 'error)))
    (insert (propertize " keys: RET jump  TAB fold  m mute  s solo  p play/stop  SPC transport  r resync"
                        'face 'vibelang-sidebar-dim-face))
    (insert "\n\n")))

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
         (soloed (eq (alist-get 'soloed group) t))
         (meter-peak (alist-get 'meter_peak group))
         (voices (alist-get 'voices group))
         (patterns (alist-get 'patterns group))
         (melodies (alist-get 'melodies group))
         (children (vibelang-sidebar--child-groups name state))
         (expanded (member name vibelang-sidebar--expanded-groups))
         (prefix (make-string (* indent 2) ?\s))
         (icon (if children (if expanded "▾" "▸") "•"))
         (node (append (list :type 'group :name name :muted muted :soloed soloed)
                       (vibelang-sidebar--find-entity-data 'group name))))
    (insert prefix)
    (insert icon " ")
    (insert (propertize name 'face (if muted
                                        'vibelang-sidebar-muted-face
                                      'vibelang-sidebar-group-face)))
    (when soloed
      (insert " ")
      (insert (propertize "[solo]" 'face 'vibelang-sidebar-playing-face)))
    (when (and vibelang-sidebar-show-meters meter-peak)
      (insert " ")
      (insert (vibelang-sidebar--format-meter meter-peak)))
    (insert "\n")
    (put-text-property (line-beginning-position 0) (1- (point)) 'vibelang-node node)
    (when expanded
      (dolist (child children)
        (vibelang-sidebar--render-group child state (1+ indent)))
      (dolist (voice-name (append voices nil))
        (vibelang-sidebar--render-voice voice-name state (1+ indent)))
      (dolist (pattern-name (append patterns nil))
        (vibelang-sidebar--render-pattern pattern-name state (1+ indent)))
      (dolist (melody-name (append melodies nil))
        (vibelang-sidebar--render-melody melody-name state (1+ indent))))))

(defun vibelang-sidebar--render-voice (name state indent)
  "Render voice NAME at INDENT level with STATE context."
  (let* ((voice-data (seq-find (lambda (v) (string= (alist-get 'name v) name))
                               (alist-get 'voices state)))
         (synth (or (alist-get 'synth voice-data) "?"))
         (muted (eq (alist-get 'muted voice-data) t))
         (soloed (eq (alist-get 'soloed voice-data) t))
         (meter-peak (alist-get 'meter_peak voice-data))
         (active-nodes (alist-get 'active_nodes voice-data))
         (active (and (listp active-nodes) (> (length active-nodes) 0)))
         (prefix (make-string (* indent 2) ?\s))
         (node (append (list :type 'voice :name name :muted muted :soloed soloed :active active)
                       (append voice-data nil))))
    (insert prefix)
    (insert (propertize (format "%s %s" (if active "◆" "◇") name)
                        'face (if muted
                                  'vibelang-sidebar-muted-face
                                'vibelang-sidebar-voice-face)))
    (insert (propertize (format " [%s]" synth) 'face 'font-lock-comment-face))
    (when soloed
      (insert (propertize " [solo]" 'face 'vibelang-sidebar-playing-face)))
    (when (and vibelang-sidebar-show-meters meter-peak)
      (insert " " )
      (insert (vibelang-sidebar--format-meter meter-peak)))
    (insert "\n")
    (put-text-property (line-beginning-position 0) (1- (point)) 'vibelang-node node)))

(defun vibelang-sidebar--render-pattern (name state indent)
  "Render pattern NAME at INDENT level with STATE context."
  (let* ((pattern-data (seq-find (lambda (p) (string= (alist-get 'name p) name))
                                 (alist-get 'patterns state)))
         (playing (eq (alist-get 'playing pattern-data) t))
         (loop-pos (alist-get 'loop_position pattern-data))
         (loop-len (alist-get 'loop_length pattern-data))
         (prefix (make-string (* indent 2) ?\s))
         (node (append (list :type 'pattern :name name :playing playing)
                       (append pattern-data nil))))
    (insert prefix)
    (insert (propertize (format "%s %s" (if playing "▶" "·") name)
                        'face 'vibelang-sidebar-pattern-face))
    (when (and playing loop-pos loop-len (> loop-len 0))
      (insert " ")
      (insert (vibelang-sidebar--format-progress loop-pos loop-len)))
    (insert "\n")
    (put-text-property (line-beginning-position 0) (1- (point)) 'vibelang-node node)))

(defun vibelang-sidebar--render-melody (name state indent)
  "Render melody NAME at INDENT level with STATE context."
  (let* ((melody-data (seq-find (lambda (m) (string= (alist-get 'name m) name))
                                (alist-get 'melodies state)))
         (playing (eq (alist-get 'playing melody-data) t))
         (loop-pos (alist-get 'loop_position melody-data))
         (loop-len (alist-get 'loop_length melody-data))
         (prefix (make-string (* indent 2) ?\s))
         (node (append (list :type 'melody :name name :playing playing)
                       (append melody-data nil))))
    (insert prefix)
    (insert (propertize (format "%s %s" (if playing "▶" "·") name)
                        'face 'vibelang-sidebar-melody-face))
    (when (and playing loop-pos loop-len (> loop-len 0))
      (insert " ")
      (insert (vibelang-sidebar--format-progress loop-pos loop-len)))
    (insert "\n")
    (put-text-property (line-beginning-position 0) (1- (point)) 'vibelang-node node)))

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
         (paused (eq (alist-get 'paused seq) t))
         (pos (alist-get 'position seq))
         (len (alist-get 'length seq))
         (looping (eq (alist-get 'looping seq) t))
         (node (append (list :type 'sequence :name name :playing playing :paused paused)
                       (append seq nil))))
    (insert " ")
    (insert (propertize (format "%s %s" (cond (paused "⏸") (playing "▶") (t "·")) name)
                        'face 'vibelang-sidebar-sequence-face))
    (insert (propertize (if looping " [loop]" " [once]") 'face 'font-lock-comment-face))
    (insert "\n")
    (put-text-property (line-beginning-position 0) (1- (point)) 'vibelang-node node)
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
         (target-type (or (alist-get 'target_type fade) "?"))
         (param (or (alist-get 'param fade) "?"))
         (current-value (alist-get 'current_value fade))
         (target-value (alist-get 'target_value fade))
         (start-value (alist-get 'start_value fade))
         (progress (alist-get 'progress fade))
         (going-up (and (numberp target-value) (numberp start-value)
                        (> target-value start-value)))
         (arrow (if going-up " ⤴ " " ⤵ "))
         (value-str (vibelang-sidebar--format-value current-value))
         (target-str (vibelang-sidebar--format-value target-value)))
    (insert arrow)
    (insert (propertize target-name 'face 'vibelang-sidebar-fade-face))
    (insert (propertize (format " (%s)." target-type) 'face 'vibelang-sidebar-dim-face))
    (insert (propertize param 'face 'vibelang-sidebar-fade-face))
    (insert "\n")
    (insert "    ")
    (insert (propertize value-str 'face 'vibelang-sidebar-fade-value-face))
    (insert (propertize (format " → %s" target-str) 'face 'font-lock-comment-face))
    (when (and progress (numberp progress) (< progress 1.0))
      (insert " ")
      (insert (vibelang-sidebar--format-fade-progress progress)))
    (insert "\n")))

(defun vibelang-sidebar--render-inspector ()
  "Render focused inspector details."
  (insert (propertize "INSPECTOR" 'face 'vibelang-sidebar-header-face))
  (insert "\n")
  (if-let ((node (vibelang-sidebar--inspector-target)))
      (let* ((type (plist-get node :type))
             (name (plist-get node :name))
             (location (vibelang-sidebar--find-node-location node))
             (origin (cond
                      ((and (eq (current-buffer) vibelang-sidebar--buffer)
                            (get-text-property (point) 'vibelang-node))
                       "sidebar")
                      ((and vibelang-sidebar-auto-inspect-point
                            (with-current-buffer (window-buffer (selected-window))
                              (derived-mode-p 'vibelang-mode)))
                       "point")
                      (t "focus"))))
        (insert (format " %s " (propertize (upcase (symbol-name type))
                                            'face 'vibelang-sidebar-inspector-key-face)))
        (insert (propertize name 'face (vibelang-sidebar--face-for-type type)))
        (insert (propertize (format "  [%s]" origin) 'face 'vibelang-sidebar-dim-face))
        (insert "\n")
        (vibelang-sidebar--render-inspector-fields type node)
        (if location
            (insert (format " %s %s:%d\n"
                            (propertize "source" 'face 'vibelang-sidebar-inspector-key-face)
                            (file-relative-name (car location) (vibelang-sidebar--project-root))
                            (cdr location)))
          (insert (format " %s %s\n"
                          (propertize "source" 'face 'vibelang-sidebar-inspector-key-face)
                          (propertize "not found in project" 'face 'vibelang-sidebar-dim-face))))
        (insert (format " %s %s\n"
                        (propertize "actions" 'face 'vibelang-sidebar-inspector-key-face)
                        (vibelang-sidebar--inspector-actions type node))))
    (insert (propertize " No node selected. Move through the sidebar or put point on an entity name in a .vibe buffer."
                        'face 'vibelang-sidebar-dim-face))
    (insert "\n")))

(defun vibelang-sidebar--render-inspector-fields (type node)
  "Render inspector fields for TYPE and NODE."
  (pcase type
    ('group
     (vibelang-sidebar--insert-field "state"
                                     (mapconcat #'identity
                                                (delq nil (list (when (plist-get node :muted) "muted")
                                                                 (when (plist-get node :soloed) "soloed")))
                                                ", "))
     (when-let ((params (alist-get 'params node)))
       (vibelang-sidebar--insert-field "params"
                                       (mapconcat (lambda (cell)
                                                    (format "%s=%s" (car cell)
                                                            (vibelang-sidebar--format-value (cdr cell))))
                                                  params ", "))))
    ('voice
     (vibelang-sidebar--insert-field "synth" (alist-get 'synth node))
     (vibelang-sidebar--insert-field "group" (alist-get 'group node))
     (vibelang-sidebar--insert-field "nodes"
                                     (if-let ((nodes (alist-get 'active_nodes node)))
                                         (mapconcat #'number-to-string nodes ", ")
                                       "idle")))
    ((or 'pattern 'melody)
     (vibelang-sidebar--insert-field "loop"
                                     (format "%.2f / %.2f"
                                             (or (alist-get 'loop_position node) 0.0)
                                             (or (alist-get 'loop_length node) 0.0)))
     (vibelang-sidebar--insert-field "state"
                                     (if (plist-get node :playing) "playing" "stopped")))
    ('sequence
     (vibelang-sidebar--insert-field "position"
                                     (format "%.2f / %.2f"
                                             (or (alist-get 'position node) 0.0)
                                             (or (alist-get 'length node) 0.0)))
     (vibelang-sidebar--insert-field "mode"
                                     (string-join
                                      (delq nil
                                            (list (if (plist-get node :playing) "playing" "stopped")
                                                  (when (plist-get node :paused) "paused")
                                                  (if (alist-get 'looping node) "loop" "once")))
                                      ", "))
     (when-let ((clips (alist-get 'active_clips node)))
       (vibelang-sidebar--insert-field
        "clips"
        (mapconcat (lambda (clip)
                     (format "%s:%s %d%%"
                             (alist-get 'type clip)
                             (alist-get 'name clip)
                             (round (* 100 (or (alist-get 'progress clip) 0.0)))))
                   clips
                   " | "))))))

(defun vibelang-sidebar--insert-field (label value)
  "Insert inspector field LABEL with VALUE."
  (when (and value (not (equal value "")))
    (insert (format " %s %s\n"
                    (propertize label 'face 'vibelang-sidebar-inspector-key-face)
                    value))))

(defun vibelang-sidebar--inspector-actions (type node)
  "Return concise action summary for TYPE and NODE."
  (pcase type
    ('group
     (format "%s · %s · RET jump"
             (if (plist-get node :muted) "m unmute" "m mute")
             (if (plist-get node :soloed) "s unsolo" "s solo")))
    ('voice
     (format "%s · %s · RET jump"
             (if (plist-get node :muted) "m unmute" "m mute")
             (if (plist-get node :active) "p stop" "p trigger")))
    ((or 'pattern 'melody 'sequence)
     (format "%s · RET jump"
             (if (plist-get node :playing) "p stop" "p start")))
    (_ "RET jump")))

;;; Inspector and entity helpers

(defun vibelang-sidebar--inspector-target ()
  "Return the best current inspector target node."
  (or (and (eq (current-buffer) vibelang-sidebar--buffer)
           (get-text-property (point) 'vibelang-node))
      vibelang-sidebar--inspector-node
      (and vibelang-sidebar-auto-inspect-point
           (vibelang-sidebar--entity-at-point))))

(defun vibelang-sidebar--entity-at-point ()
  "Best-effort runtime entity lookup for the current point."
  (when (and vibelang-sidebar--project-state
             (derived-mode-p 'vibelang-mode))
    (let ((candidate (or (vibelang-sidebar--string-at-point)
                         (thing-at-point 'symbol t))))
      (when (and candidate (not (string-empty-p candidate)))
        (vibelang-sidebar--find-entity-by-name candidate)))))

(defun vibelang-sidebar--string-at-point ()
  "Return current string contents without text properties, if inside a string."
  (when-let* ((bounds (bounds-of-thing-at-point 'string))
              (raw (buffer-substring-no-properties (car bounds) (cdr bounds))))
    (string-trim raw "\"" "\"")))

(defun vibelang-sidebar--find-entity-by-name (name)
  "Find runtime entity named NAME in the current project state."
  (let ((priority '(group voice pattern melody sequence)))
    (seq-some
     (lambda (type)
       (vibelang-sidebar--find-entity-data type name))
     priority)))

(defun vibelang-sidebar--find-entity-data (type name)
  "Return a sidebar node plist for TYPE/NAME from playback state, or nil."
  (when-let* ((state vibelang-sidebar--project-state)
              (section (alist-get (intern (concat (symbol-name type) "s")) state))
              (entry (seq-find (lambda (it)
                                 (string= (alist-get 'name it) name))
                               section)))
    (append (list :type type :name name)
            (append entry nil)
            (when (eq type 'voice)
              (list :active (let ((nodes (alist-get 'active_nodes entry)))
                              (and (listp nodes) (> (length nodes) 0))))))))

(defun vibelang-sidebar--child-groups (parent-name state)
  "Return child groups of PARENT-NAME from STATE."
  (seq-filter (lambda (group)
                (string= (alist-get 'parent group) parent-name))
              (alist-get 'groups state)))

(defun vibelang-sidebar--face-for-type (type)
  "Return primary display face for TYPE."
  (pcase type
    ('group 'vibelang-sidebar-group-face)
    ('voice 'vibelang-sidebar-voice-face)
    ('pattern 'vibelang-sidebar-pattern-face)
    ('melody 'vibelang-sidebar-melody-face)
    ('sequence 'vibelang-sidebar-sequence-face)
    (_ 'default)))

(defun vibelang-sidebar--project-root ()
  "Return best project root for sidebar lookups."
  (or (when-let* ((proj (project-current nil)))
        (cond
         ((fboundp 'project-root)
          (project-root proj))
         ((fboundp 'project-roots)
          (car (funcall #'project-roots proj)))))
      default-directory))

(defun vibelang-sidebar--find-node-location (node)
  "Return cons cell (FILE . LINE) for NODE by scanning project files."
  (let* ((root (vibelang-sidebar--project-root))
         (name (plist-get node :name))
         (regexp (pcase (plist-get node :type)
                   ('group (format "define_group(\\\"%s\\\"" (regexp-quote name)))
                   ('voice (format "voice(\\\"%s\\\"" (regexp-quote name)))
                   ('pattern (format "pattern(\\\"%s\\\"" (regexp-quote name)))
                   ('melody (format "melody(\\\"%s\\\"" (regexp-quote name)))
                   ('sequence (format "sequence(\\\"%s\\\"" (regexp-quote name)))
                   (_ nil))))
    (when (and root regexp)
      (catch 'found
        (dolist (file (directory-files-recursively root "\\.vibe\\'" nil nil))
          (with-temp-buffer
            (insert-file-contents file)
            (goto-char (point-min))
            (when (re-search-forward regexp nil t)
              (throw 'found (cons file (line-number-at-pos (match-beginning 0)))))))))))

(defun vibelang-sidebar--entity-post (type name action &optional body)
  "POST ACTION for TYPE/NAME, optionally sending BODY JSON."
  (let* ((path (format "/%ss/%s/%s"
                       (symbol-name type)
                       (url-hexify-string name)
                       action))
         (result (vibelang-http-request "POST" path body)))
    (if (plist-get result :ok)
        (progn
          (message "VibeLang %s %s: %s" (symbol-name type) name action)
          (when (memq type '(group voice pattern melody sequence))
            (vibelang-ws-request-resync (format "%s %s" action name))))
      (user-error "%s" (or (plist-get result :error)
                            (format "Request failed: %s" path))))))

(defun vibelang-sidebar--format-age (timestamp)
  "Format relative age for TIMESTAMP."
  (if timestamp
      (format "%.1fs" (- (float-time) timestamp))
    "never"))

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

(defun vibelang-sidebar--meter-db (peak)
  "Convert PEAK amplitude to a clamped dB value."
  (max -60.0 (if (> peak 0.0001) (* 20 (log peak 10)) -60.0)))

(defun vibelang-sidebar--meter-state-face ()
  "Return an accent face for the current connection state."
  (pcase (vibelang-ws-connection-state)
    ('stale 'warning)
    ((or 'connected 'reconnecting) 'vibelang-sidebar-dim-face)
    ('protocol-mismatch 'error)
    (_ 'vibelang-sidebar-progress-percent-face)))

(defun vibelang-sidebar--meter-segment-face (index width synced)
  "Return face for meter segment INDEX in WIDTH columns.
When SYNCED is nil, return a state-aware fallback face."
  (if (not synced)
      (vibelang-sidebar--meter-state-face)
    (cond
     ((< index (/ width 2)) 'vibelang-sidebar-meter-low-face)
     ((< index (* 3 (/ width 4))) 'vibelang-sidebar-meter-mid-face)
     (t 'vibelang-sidebar-meter-high-face))))

(defun vibelang-sidebar--format-meter (peak)
  "Format PEAK level as a compact bar with dB readout."
  (let* ((width (max 6 vibelang-sidebar-meter-width))
         (db (vibelang-sidebar--meter-db peak))
         (normalized (/ (+ db 60.0) 60.0))
         (filled (max 0 (min width (round (* width normalized)))))
         (synced (eq (vibelang-ws-connection-state) 'synced))
         (bar (apply #'concat
                     (mapcar (lambda (idx)
                               (propertize (if (< idx filled) "█" "·")
                                           'face (if (< idx filled)
                                                     (vibelang-sidebar--meter-segment-face idx width synced)
                                                   'vibelang-sidebar-progress-empty-face)))
                             (number-sequence 0 (1- width)))))
         (label (if (<= db -59.5)
                    "-inf"
                  (format "%+.0f" db))))
    (concat "[" bar "]"
            (propertize (format " %4sdB" label)
                        'face (vibelang-sidebar--meter-state-face)))))

(defun vibelang-sidebar--format-progress (pos length)
  "Format progress bar for POS out of LENGTH beats."
  (let* ((width 10)
         (percent (if (> length 0)
                      (* 100 (/ (float pos) length))
                    0))
         (filled (if (> length 0)
                     (floor (* width (/ (float pos) length)))
                   0))
         (filled (min filled (1- width)))
         (empty (- width filled 1))
         (filled-str (if (> filled 0)
                         (propertize (make-string filled ?━)
                                     'face 'vibelang-sidebar-progress-filled-face)
                       ""))
         (head-str (propertize "●" 'face 'vibelang-sidebar-progress-head-face))
         (empty-str (if (> empty 0)
                        (propertize (make-string empty ?─)
                                    'face 'vibelang-sidebar-progress-empty-face)
                      ""))
         (percent-str (propertize (format " %3.0f%%" percent)
                                  'face 'vibelang-sidebar-progress-percent-face)))
    (concat "╶" filled-str head-str empty-str "╴" percent-str)))

(provide 'vibelang-sidebar)
;;; vibelang-sidebar.el ends here
