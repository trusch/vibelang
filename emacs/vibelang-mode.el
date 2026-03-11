;;; vibelang-mode.el --- Major mode for VibeLang files -*- lexical-binding: t -*-

;; Copyright (C) 2024 VibeLang Authors
;; Author: VibeLang Team
;; Version: 0.1.0
;; Package-Requires: ((emacs "27.1") (websocket "1.14"))
;; Keywords: languages, music, multimedia
;; URL: https://github.com/vibelang/vibelang

;;; Commentary:
;; Major mode for editing VibeLang (.vibe) files with:
;; - Full LSP support (completion, hover, diagnostics, etc.)
;; - Real-time playback visualization via WebSocket
;;
;; Features:
;; - LSP integration via eglot (Emacs 29+) or lsp-mode
;; - Code completion with snippets
;; - Hover documentation
;; - Go-to-definition and find references
;; - Diagnostics (syntax/semantic errors)
;; - Semantic token highlighting
;; - Inlay hints (parameter names, timing info)
;; - Real-time beat position indicators
;; - Sequence progress visualization
;; - Mode-line transport display
;;
;; Usage:
;; 1. Open a .vibe file (mode activates automatically)
;; 2. LSP features work immediately if vibelang is in PATH
;; 3. For live visualization: M-x vibelang-connect (C-c C-c)

;;; Code:

(require 'vibelang-syntax)
(require 'vibelang-lsp)
(require 'vibelang-websocket)
(require 'vibelang-visualization)
(require 'vibelang-sidebar)

(defgroup vibelang nil
  "VibeLang live coding environment."
  :group 'languages
  :prefix "vibelang-")

(defcustom vibelang-ws-host "127.0.0.1"
  "VibeLang WebSocket server host.
Use 127.0.0.1 instead of localhost to avoid IPv6 issues."
  :type 'string
  :group 'vibelang)

(defcustom vibelang-ws-port 1606
  "VibeLang WebSocket server port."
  :type 'integer
  :group 'vibelang)

(defcustom vibelang-auto-connect 'if-running
  "Whether to automatically connect to VibeLang when opening a .vibe file.
Possible values:
  nil         - Never auto-connect
  t           - Always try to auto-connect
  `if-running' - Auto-connect only if VibeLang is already running (default)"
  :type '(choice (const :tag "Never" nil)
                 (const :tag "Always" t)
                 (const :tag "Only if running" if-running))
  :group 'vibelang)

(defcustom vibelang-visualization-enabled t
  "Whether to show beat position visualization by default."
  :type 'boolean
  :group 'vibelang)

(defcustom vibelang-interpolation-fps 30
  "Frames per second for beat position interpolation (1-60)."
  :type 'integer
  :group 'vibelang)

(defcustom vibelang-enable-lsp t
  "Whether to enable LSP support when opening .vibe files.
Requires eglot (Emacs 29+) or lsp-mode."
  :type 'boolean
  :group 'vibelang)

(defcustom vibelang-enable-header-line t
  "Whether to show transport header-line in VibeLang buffers."
  :type 'boolean
  :group 'vibelang)

(defcustom vibelang-sidebar-on-connect nil
  "Whether to automatically open sidebar when connecting to VibeLang."
  :type 'boolean
  :group 'vibelang)

(defcustom vibelang-executable "vibe"
  "Path to the vibelang executable (usually 'vibe' or 'vibelang')."
  :type 'string
  :group 'vibelang)

(defcustom vibelang-runtime-args '("run")
  "Default arguments for starting the VibeLang runtime.
The script file will be appended to this list.
Note: Watch mode and API server are enabled by default in vibe."
  :type '(repeat string)
  :group 'vibelang)

(defvar vibelang--runtime-process nil
  "The VibeLang runtime process started from Emacs.")

(defvar vibelang--runtime-buffer-name "*vibelang-runtime*"
  "Buffer name for VibeLang runtime output.")

(defvar vibelang-mode-map
  (let ((map (make-sparse-keymap)))
    ;; Runtime control
    (define-key map (kbd "C-c C-c") #'vibelang-connect)
    (define-key map (kbd "C-c C-d") #'vibelang-disconnect)
    (define-key map (kbd "C-c C-k") #'vibelang-stop-runtime)
    (define-key map (kbd "C-c r s") #'vibelang-start-runtime)
    (define-key map (kbd "C-c r k") #'vibelang-stop-runtime)
    (define-key map (kbd "C-c r r") #'vibelang-restart-runtime)
    ;; Transport
    (define-key map (kbd "C-c C-s") #'vibelang-transport-start)
    (define-key map (kbd "C-c C-x") #'vibelang-transport-stop)
    (define-key map (kbd "C-c C-v") #'vibelang-toggle-visualization)
    (define-key map (kbd "C-c C-r") #'vibelang-reload-script)
    ;; Sidebar
    (define-key map (kbd "C-c C-b") #'vibelang-sidebar-toggle)
    (define-key map (kbd "C-c b o") #'vibelang-sidebar-open)
    ;; Transport header
    (define-key map (kbd "C-c C-h") #'vibelang-toggle-header-line)
    ;; LSP
    (define-key map (kbd "C-c C-l") #'vibelang-lsp-enable)
    (define-key map (kbd "C-c l r") #'vibelang-lsp-restart)
    ;; Diagnostics
    (define-key map (kbd "C-c C-?") #'vibelang-ws-diagnose)
    map)
  "Keymap for `vibelang-mode'.")

(defvar vibelang-mode-syntax-table
  (let ((st (make-syntax-table)))
    ;; C-style comments
    (modify-syntax-entry ?/ ". 124b" st)
    (modify-syntax-entry ?* ". 23" st)
    (modify-syntax-entry ?\n "> b" st)
    ;; Strings
    (modify-syntax-entry ?\" "\"" st)
    (modify-syntax-entry ?` "\"" st)
    ;; Underscores are word constituents
    (modify-syntax-entry ?_ "w" st)
    st)
  "Syntax table for `vibelang-mode'.")

;;;###autoload
(define-derived-mode vibelang-mode prog-mode "VibeLang"
  "Major mode for editing VibeLang (.vibe) files.

VibeLang is a domain-specific language for live music coding.
This mode provides syntax highlighting and real-time playback
visualization through WebSocket connection to the VibeLang runtime.

\\{vibelang-mode-map}"
  :group 'vibelang
  :syntax-table vibelang-mode-syntax-table

  ;; Comments
  (setq-local comment-start "// ")
  (setq-local comment-end "")
  (setq-local comment-start-skip "\\(?://+\\|/\\*+\\)\\s *")

  ;; Font lock
  (setq-local font-lock-defaults '(vibelang-font-lock-keywords nil nil nil nil))

  ;; Velocity/pitch colorization for pattern strings
  (when (fboundp 'vibelang-syntax-propertize)
    (add-hook 'font-lock-extend-after-change-region-function
              (lambda (beg end _old-len)
                (cons (save-excursion (goto-char beg) (line-beginning-position))
                      (save-excursion (goto-char end) (line-end-position))))
              nil t)
    (add-hook 'after-change-functions
              (lambda (beg end _len)
                (vibelang-syntax-propertize
                 (save-excursion (goto-char beg) (line-beginning-position))
                 (save-excursion (goto-char end) (line-end-position))))
              nil t)
    ;; Initial colorization
    (vibelang-syntax-propertize (point-min) (point-max)))

  ;; Indentation (basic - could be improved)
  (setq-local indent-tabs-mode nil)
  (setq-local tab-width 4)

  ;; Enable visualization minor mode
  (when vibelang-visualization-enabled
    (vibelang-visualization-mode 1))

  ;; Enable LSP if configured
  (when vibelang-enable-lsp
    (vibelang-lsp-enable))

  ;; Enable transport header-line
  (when vibelang-enable-header-line
    (vibelang--setup-header-line))

  ;; Auto-connect WebSocket if configured
  (vibelang--maybe-auto-connect))

;;;###autoload
(add-to-list 'auto-mode-alist '("\\.vibe\\'" . vibelang-mode))

;;; Interactive commands


(defun vibelang--server-running-p ()
  "Return non-nil if VibeLang server is accepting connections.
This does a quick TCP probe without establishing a full WebSocket connection."
  (condition-case err
      (let ((proc (make-network-process
                   :name "vibelang-probe"
                   :host vibelang-ws-host
                   :service vibelang-ws-port
                   :nowait nil
                   :coding 'binary)))
        (when proc
          (delete-process proc)
          t))
    (file-error nil)  ; Connection refused, host unreachable, etc.
    (error
     ;; Log unexpected errors for debugging
     (message "VibeLang probe error: %s" (error-message-string err))
     nil)))

(defun vibelang--maybe-auto-connect ()
  "Auto-connect based on `vibelang-auto-connect' setting.
Called when entering `vibelang-mode'."
  (cond
   ;; Never auto-connect
   ((null vibelang-auto-connect)
    nil)
   ;; Always auto-connect (existing behavior)
   ((eq vibelang-auto-connect t)
    (vibelang-connect))
   ;; Auto-connect only if server is already running
   ((eq vibelang-auto-connect 'if-running)
    ;; Run probe asynchronously to avoid blocking mode init
    (run-at-time 0.1 nil #'vibelang--try-auto-connect))))

(defun vibelang--try-auto-connect ()
  "Attempt auto-connect if server is running."
  (when (vibelang--server-running-p)
    (message "VibeLang server detected at %s:%d, connecting..."
             vibelang-ws-host vibelang-ws-port)
    (vibelang-ws-connect vibelang-ws-host vibelang-ws-port)))

(defun vibelang-connect ()
  "Connect to VibeLang WebSocket server.
VibeLang must already be running. Use `vibelang-start-runtime' to start it."
  (interactive)
  ;; First, check if server is accepting connections (TCP probe)
  (if (vibelang--server-running-p)
      ;; Server is running - connect to it
      (progn
        (message "Connecting to VibeLang at %s:%d..."
                 vibelang-ws-host vibelang-ws-port)
        (setq vibelang--connection-retry-count 0)
        (vibelang-ws-connect vibelang-ws-host vibelang-ws-port)
        ;; Check connection status after a moment
        (run-at-time 0.5 nil #'vibelang--check-quick-connection))
    ;; Server not running - inform the user
    (message "VibeLang is not running. Start it with: vibe run your-script.vibe")))

(defun vibelang--check-quick-connection ()
  "Check if connection succeeded, retry if server is still running."
  (cond
   ;; Connected successfully
   ((vibelang-ws-connected-p)
    (message "Connected to VibeLang!")
    (when vibelang-sidebar-on-connect
      (vibelang-sidebar-open)))
   ;; Server is running but WebSocket not connected yet - retry
   ((vibelang--server-running-p)
    (setq vibelang--connection-retry-count (1+ vibelang--connection-retry-count))
    (if (< vibelang--connection-retry-count vibelang--max-connection-retries)
        (progn
          (message "Retrying connection... (attempt %d)" vibelang--connection-retry-count)
          (vibelang-ws-connect vibelang-ws-host vibelang-ws-port)
          (run-at-time 0.3 nil #'vibelang--check-quick-connection))
      (message "Failed to connect after %d attempts. Server is running but WebSocket failed."
               vibelang--max-connection-retries)))
   ;; Server stopped running
   (t
    (message "VibeLang server is not running"))))

(defvar vibelang--api-ready nil
  "Non-nil when API server has reported it's ready.")

(defvar vibelang--connection-retry-count 0
  "Number of connection retries attempted.")

(defvar vibelang--max-connection-retries 10
  "Maximum number of connection retry attempts.")

(defun vibelang-start-runtime (script)
  "Start VibeLang runtime with SCRIPT file."
  (interactive
   (list (or (buffer-file-name)
             (read-file-name "Script to run: " nil nil t nil
                             (lambda (f) (string-match-p "\\.vibe\\'" f))))))
  ;; Check if executable exists
  (unless (executable-find vibelang-executable)
    (error "VibeLang executable '%s' not found in PATH. Install VibeLang or set vibelang-executable"
           vibelang-executable))
  ;; Check if already running
  (when (and vibelang--runtime-process
             (process-live-p vibelang--runtime-process))
    (if (yes-or-no-p "VibeLang is already running. Restart? ")
        (vibelang-stop-runtime)
      (user-error "Aborted")))
  ;; Reset connection state
  (setq vibelang--api-ready nil)
  (setq vibelang--connection-retry-count 0)
  ;; Build command
  (let* ((script-path (expand-file-name script))
         (args (append vibelang-runtime-args (list script-path)))
         (buf (get-buffer-create vibelang--runtime-buffer-name)))
    ;; Verify script exists
    (unless (file-exists-p script-path)
      (error "Script file not found: %s" script-path))
    ;; Clear previous output
    (with-current-buffer buf
      (erase-buffer)
      (special-mode)
      (let ((inhibit-read-only t))
        (insert (format "╔══════════════════════════════════════════════════════════════╗\n"))
        (insert (format "║  VibeLang Runtime                                            ║\n"))
        (insert (format "╚══════════════════════════════════════════════════════════════╝\n\n"))
        (insert (format "Command: %s %s\n" vibelang-executable (string-join args " ")))
        (insert (format "Script:  %s\n" script-path))
        (insert (format "Started: %s\n\n" (format-time-string "%Y-%m-%d %H:%M:%S")))
        (insert "─────────────────────────────────────────────────────────────────\n\n")))
    ;; Start process
    (setq vibelang--runtime-process
          (make-process
           :name "vibelang-runtime"
           :buffer buf
           :command (cons vibelang-executable args)
           :sentinel #'vibelang--runtime-sentinel
           :filter #'vibelang--runtime-filter))
    ;; Show buffer in a small window
    (display-buffer buf '((display-buffer-at-bottom)
                          (window-height . 12)))
    (message "Starting VibeLang runtime with %s..." (file-name-nondirectory script))
    ;; Start connection attempt loop after brief delay for server startup
    (run-at-time 0.8 nil #'vibelang--try-connect)))

(defun vibelang--runtime-filter (proc string)
  "Process filter for VibeLang runtime PROC receiving STRING."
  (when (buffer-live-p (process-buffer proc))
    (with-current-buffer (process-buffer proc)
      (let ((inhibit-read-only t)
            (moving (= (point) (process-mark proc))))
        (save-excursion
          (goto-char (process-mark proc))
          (insert string)
          (set-marker (process-mark proc) (point)))
        (when moving
          (goto-char (process-mark proc)))))
    ;; Check for API server ready message
    (when (string-match-p "HTTP API server started\\|HTTP API server starting" string)
      (setq vibelang--api-ready t)
      (message "VibeLang API server is ready"))))

(defun vibelang--try-connect ()
  "Attempt to connect to VibeLang WebSocket with retries."
  ;; Check if process is still running
  (if (not (and vibelang--runtime-process
                (process-live-p vibelang--runtime-process)))
      (message "VibeLang runtime failed to start. Check *vibelang-runtime* buffer.")
    ;; Check if already connected
    (if (vibelang-ws-connected-p)
        (progn
          (message "Connected to VibeLang!")
          (when vibelang-sidebar-on-connect
            (vibelang-sidebar-open)))
      ;; Try to connect
      (condition-case err
          (progn
            (when (> vibelang--connection-retry-count 2)
              (message "Waiting for VibeLang... (attempt %d)"
                       (1+ vibelang--connection-retry-count)))
            (vibelang-ws-connect vibelang-ws-host vibelang-ws-port)
            ;; Check connection status after a short delay
            (run-at-time 0.3 nil #'vibelang--check-connection))
        (error
         (vibelang--handle-connection-failure (error-message-string err)))))))

(defun vibelang--check-connection ()
  "Check if WebSocket connection succeeded, retry if not."
  (if (vibelang-ws-connected-p)
      (progn
        (message "Connected to VibeLang!")
        (when vibelang-sidebar-on-connect
          (vibelang-sidebar-open)))
    ;; Not connected, retry if we haven't exceeded max retries
    (vibelang--handle-connection-failure nil)))

(defun vibelang--handle-connection-failure (&optional error-msg)
  "Handle connection failure, retry or give up based on retry count.
Optional ERROR-MSG provides additional context."
  (setq vibelang--connection-retry-count (1+ vibelang--connection-retry-count))
  (if (>= vibelang--connection-retry-count vibelang--max-connection-retries)
      ;; Give up
      (progn
        (message "VibeLang started but WebSocket connection failed after %d attempts. Check *vibelang-runtime* buffer."
                 vibelang--max-connection-retries)
        (when error-msg
          (message "Last error: %s" error-msg)))
    ;; Retry with short delay (0.3s between attempts)
    (run-at-time 0.3 nil #'vibelang--try-connect)))

(defun vibelang-stop-runtime ()
  "Stop the VibeLang runtime process."
  (interactive)
  (when (and vibelang--runtime-process
             (process-live-p vibelang--runtime-process))
    (vibelang-ws-disconnect)
    (interrupt-process vibelang--runtime-process)
    (sit-for 0.5)
    (when (process-live-p vibelang--runtime-process)
      (kill-process vibelang--runtime-process))
    (setq vibelang--api-ready nil)
    (message "VibeLang runtime stopped")))

(defun vibelang--runtime-sentinel (process event)
  "Handle VibeLang runtime PROCESS EVENT."
  (let ((event (string-trim event)))
    (cond
     ((string-match-p "finished\\|exited" event)
      (message "VibeLang runtime exited: %s" event)
      (vibelang-ws-disconnect))
     ((string-match-p "killed\\|terminated" event)
      (message "VibeLang runtime terminated")))))

(defun vibelang-restart-runtime ()
  "Restart the VibeLang runtime with the current script."
  (interactive)
  (let ((script (buffer-file-name)))
    (if (and script (string-match-p "\\.vibe\\'" script))
        (progn
          (vibelang-stop-runtime)
          (sit-for 0.5)
          (vibelang-start-runtime script))
      (message "No .vibe file in current buffer"))))

(defun vibelang-runtime-running-p ()
  "Return non-nil if the VibeLang runtime is running."
  (and vibelang--runtime-process
       (process-live-p vibelang--runtime-process)))

(defun vibelang-disconnect ()
  "Disconnect from VibeLang WebSocket server."
  (interactive)
  (vibelang-ws-disconnect))

(defun vibelang-transport-start ()
  "Start transport playback."
  (interactive)
  (vibelang-ws-send-command "transport.start"))

(defun vibelang-transport-stop ()
  "Stop transport playback."
  (interactive)
  (vibelang-ws-send-command "transport.stop"))

(defun vibelang-toggle-visualization ()
  "Toggle playback visualization on/off."
  (interactive)
  (vibelang-visualization-mode (if vibelang-visualization-mode -1 1))
  (message "Visualization %s" (if vibelang-visualization-mode "enabled" "disabled")))

(defun vibelang-reload-script ()
  "Reload the current script (requires HTTP API)."
  (interactive)
  (let ((file (buffer-file-name)))
    (when file
      (save-buffer)
      (vibelang-ws-send-command "eval.reload" `((file . ,file))))))

(defun vibelang-toggle-header-line ()
  "Toggle the transport header-line display."
  (interactive)
  (if header-line-format
      (setq-local header-line-format nil)
    (vibelang--setup-header-line))
  (message "Transport header %s" (if header-line-format "enabled" "disabled")))

;;; Transport header-line

(defvar-local vibelang--header-transport-state nil
  "Current transport state for header-line display.")

(defface vibelang-header-playing-face
  '((((class color) (background dark))
     :foreground "#00ff00" :weight bold)
    (((class color) (background light))
     :foreground "#00aa00" :weight bold))
  "Face for playing indicator in header."
  :group 'vibelang)

(defface vibelang-header-stopped-face
  '((((class color) (background dark))
     :foreground "#ff6666")
    (((class color) (background light))
     :foreground "#cc0000"))
  "Face for stopped indicator in header."
  :group 'vibelang)

(defface vibelang-header-beat-face
  '((((class color) (background dark))
     :foreground "#e5c07b" :weight bold)
    (((class color) (background light))
     :foreground "#c18401" :weight bold))
  "Face for beat display in header."
  :group 'vibelang)

(defface vibelang-header-bpm-face
  '((((class color) (background dark))
     :foreground "#61afef")
    (((class color) (background light))
     :foreground "#4078f2"))
  "Face for BPM display in header."
  :group 'vibelang)

(defun vibelang--setup-header-line ()
  "Set up the transport header-line."
  (setq-local header-line-format
              '(:eval (vibelang--format-header-line))))

(defun vibelang--format-header-line ()
  "Format the transport header-line content."
  (let* ((state vibelang--header-transport-state)
         (transport (and state (alist-get 'transport state)))
         (connected (vibelang-ws-connected-p)))
    (if (not connected)
        (concat " " (propertize "DISCONNECTED" 'face 'vibelang-header-stopped-face)
                "  Press C-c C-c to connect")
      (if (not transport)
          (concat " " (propertize "WAITING" 'face 'font-lock-comment-face)
                  "  Waiting for playback data...")
        (let* ((playing (eq (alist-get 'playing transport) t))
               (bar (or (alist-get 'bar transport) 0))
               (beat-in-bar (or (alist-get 'beat_in_bar transport) 0))
               (bpm (or (alist-get 'bpm transport) 120))
               (time-sig (alist-get 'time_sig transport))
               (num (or (car time-sig) 4))
               (denom (or (cadr time-sig) 4)))
          (concat
           ;; Play/pause indicator
           " "
           (if playing
               (propertize "▶ PLAYING" 'face 'vibelang-header-playing-face)
             (propertize "■ STOPPED" 'face 'vibelang-header-stopped-face))
           "  │  "
           ;; Position
           (propertize (format "Bar %d.%d" (1+ bar) (1+ beat-in-bar))
                       'face 'vibelang-header-beat-face)
           "  │  "
           ;; BPM
           (propertize (format "♩ = %.0f" bpm)
                       'face 'vibelang-header-bpm-face)
           "  │  "
           ;; Time signature
           (format "%d/%d" num denom)
           "  │  "
           ;; Progress bar (visual beat within bar)
           (vibelang--format-header-progress beat-in-bar num)))))))

(defun vibelang--format-header-progress (beat-in-bar beats-per-bar)
  "Format progress bar showing BEAT-IN-BAR out of BEATS-PER-BAR."
  (let* ((width (* beats-per-bar 2))
         (filled (* (1+ beat-in-bar) 2)))
    (concat "["
            (propertize (make-string (min filled width) ?█)
                        'face 'vibelang-header-beat-face)
            (make-string (max 0 (- width filled)) ?░)
            "]")))

(defun vibelang--update-header-transport (state)
  "Update header transport state with STATE."
  (dolist (buf (buffer-list))
    (with-current-buffer buf
      (when (derived-mode-p 'vibelang-mode)
        (setq vibelang--header-transport-state state)
        (force-mode-line-update)))))

;; Hook into playback state updates
(add-hook 'vibelang--playback-state-hook #'vibelang--update-header-transport)

;;; Mode-line indicator

(defvar vibelang--mode-line-string ""
  "Mode line string showing VibeLang playback state.")

(defun vibelang--update-mode-line (state)
  "Update mode line with playback STATE."
  (if state
      (let* ((transport (alist-get 'transport state))
             (playing (eq (alist-get 'playing transport) t))
             (bar (alist-get 'bar transport))
             (beat-in-bar (alist-get 'beat_in_bar transport))
             (bpm (alist-get 'bpm transport)))
        (setq vibelang--mode-line-string
              (if playing
                  (format " [%s %d.%d @ %.0f]"
                          (propertize ">" 'face '(:foreground "green"))
                          (1+ bar) (1+ beat-in-bar) bpm)
                (format " [%s]"
                        (propertize "||" 'face '(:foreground "red"))))))
    (setq vibelang--mode-line-string ""))
  (force-mode-line-update t))

;; Add to global mode string
(unless (member '(:eval vibelang--mode-line-string) global-mode-string)
  (setq global-mode-string
        (append global-mode-string '((:eval vibelang--mode-line-string)))))

(provide 'vibelang-mode)
;;; vibelang-mode.el ends here
