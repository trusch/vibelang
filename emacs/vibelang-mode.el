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

(require 'subr-x)
(require 'cl-lib)
(require 'vibelang-syntax)
(require 'vibelang-indent)
(require 'vibelang-lsp)
(require 'vibelang-websocket)
(require 'vibelang-visualization)
(require 'vibelang-sidebar)
(require 'vibelang-transient)
(require 'vibelang-templates)

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
  "Path to the vibelang executable (usually `vibe' or `vibelang')."
  :type 'string
  :group 'vibelang)

(defcustom vibelang-runtime-args '("run")
  "Default arguments for starting the VibeLang runtime.
The script file will be appended to this list.
Note: Watch mode and API server are enabled by default in vibe."
  :type '(repeat string)
  :group 'vibelang)

(defcustom vibelang-auto-reload-on-save nil
  "When non-nil, reload the script automatically on every save."
  :type 'boolean
  :group 'vibelang)

(defvar vibelang--runtime-process nil
  "The VibeLang runtime process started from Emacs.")

(defvar vibelang--runtime-buffer-name "*vibelang-runtime*"
  "Buffer name for VibeLang runtime output.")

(defvar vibelang--api-ready nil
  "Non-nil when API server has reported it's ready.")

(defvar vibelang--connection-retry-count 0
  "Number of connection retries attempted.")

(defvar vibelang--max-connection-retries 10
  "Maximum number of connection retry attempts.")

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
    (define-key map (kbd "C-c C-t") #'vibelang-tap-tempo)
    (define-key map (kbd "C-c C-v") #'vibelang-toggle-visualization)
    (define-key map (kbd "C-c C-r") #'vibelang-reload-script)
    (define-key map (kbd "C-c +") #'vibelang-bpm-nudge-up)
    (define-key map (kbd "C-c -") #'vibelang-bpm-nudge-down)
    (define-key map (kbd "C-c C-q") #'vibelang-set-bpm)
    ;; Eval / cockpit
    (define-key map (kbd "C-c C-e") #'vibelang-eval-dwim)
    (define-key map (kbd "C-c e b") #'vibelang-eval-buffer)
    (define-key map (kbd "C-c e r") #'vibelang-eval-region)
    (define-key map (kbd "C-c e l") #'vibelang-eval-line)
    (define-key map (kbd "C-c e p") #'vibelang-eval-prompt)
    (define-key map (kbd "C-c .") #'vibelang-cockpit-open)
    (define-key map (kbd "C-c C-.") #'vibelang-menu)
    ;; Sidebar
    (define-key map (kbd "C-c C-b") #'vibelang-sidebar-toggle)
    (define-key map (kbd "C-c b o") #'vibelang-sidebar-open)
    (define-key map (kbd "C-c b i") #'vibelang-sidebar-focus-current-entity)
    (define-key map (kbd "C-c b r") #'vibelang-sidebar-request-resync)
    ;; Semantic navigation / structural editing
    (define-key map (kbd "M-n") #'vibelang-forward-entity)
    (define-key map (kbd "M-p") #'vibelang-backward-entity)
    (define-key map (kbd "C-M-h") #'vibelang-mark-entity)
    (define-key map (kbd "C-c n r") #'vibelang-rename-entity-definition)
    (define-key map (kbd "C-c n c") #'vibelang-clone-entity)
    (define-key map (kbd "C-c n .") #'vibelang-current-entity)
    ;; Transport header
    (define-key map (kbd "C-c C-h") #'vibelang-help)
    (define-key map (kbd "C-c h t") #'vibelang-toggle-header-line)
    ;; Mute / solo / inspect
    (define-key map (kbd "C-c m") #'vibelang-mute-at-point)
    (define-key map (kbd "C-c s") #'vibelang-solo-at-point)
    (define-key map (kbd "C-c C-i") #'vibelang-describe-entity)
    ;; Live param editing
    (define-key map (kbd "C-c C-p") #'vibelang-edit-param-at-point)
    (define-key map (kbd "C-c P")   #'vibelang-tweak-param-at-point)
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

  ;; Velocity/pitch colorization for pattern strings via syntax-propertize
  (when (fboundp 'vibelang-syntax-propertize)
    (setq-local syntax-propertize-function #'vibelang-syntax-propertize))

  ;; Indentation
  (setq-local indent-line-function #'vibelang-indent-line)
  (setq-local tab-width vibelang-indent-offset)
  (setq-local indent-tabs-mode nil)

  ;; Semantic navigation integrates with standard defun commands.
  (setq-local beginning-of-defun-function #'vibelang-beginning-of-entity)
  (when (boundp 'end-of-defun-function)
    (setq-local end-of-defun-function #'vibelang-end-of-entity))
  (setq-local add-log-current-defun-function #'vibelang-current-defun-name)

  ;; imenu integration
  (setq-local imenu-create-index-function #'vibelang-imenu-create-index)

  ;; Enable visualization minor mode
  (when vibelang-visualization-enabled
    (vibelang-visualization-mode 1))

  ;; Enable LSP if configured
  (when vibelang-enable-lsp
    (vibelang-lsp-enable))

  ;; Enable transport header-line
  (when vibelang-enable-header-line
    (vibelang--setup-header-line))

  ;; Flash the buffer on save to confirm hot-reload was triggered
  (add-hook 'after-save-hook #'vibelang--on-after-save nil t)

  ;; Auto-connect WebSocket if configured
  (vibelang--maybe-auto-connect)

  ;; Load snippet templates (tempel or yasnippet)
  (vibelang-setup-templates))

;;;###autoload
(add-to-list 'auto-mode-alist '("\\.vibe\\'" . vibelang-mode))

(defvar vibelang-cockpit-buffer-name "*VibeLang Cockpit*"
  "Buffer name for the VibeLang cockpit.")

(defvar vibelang-cockpit-mode-map
  (let ((map (make-sparse-keymap)))
    (define-key map (kbd "g") #'vibelang-cockpit-refresh)
    (define-key map (kbd "c") #'vibelang-connect)
    (define-key map (kbd "d") #'vibelang-disconnect)
    (define-key map (kbd "s") #'vibelang-start-runtime)
    (define-key map (kbd "k") #'vibelang-stop-runtime)
    (define-key map (kbd "R") #'vibelang-restart-runtime)
    (define-key map (kbd "r") #'vibelang-reload-script)
    (define-key map (kbd "e") #'vibelang-eval-dwim)
    (define-key map (kbd "p") #'vibelang-transport-start)
    (define-key map (kbd "x") #'vibelang-transport-stop)
    (define-key map (kbd "b") #'vibelang-sidebar-toggle)
    (define-key map (kbd "?") #'vibelang-ws-diagnose)
    map)
  "Keymap for `vibelang-cockpit-mode'.")

(define-derived-mode vibelang-cockpit-mode special-mode "VibeLang-Cockpit"
  "Major mode for the VibeLang live coding cockpit."
  :group 'vibelang
  (setq-local truncate-lines nil)
  (setq-local buffer-read-only t))

;;; Entity navigation and structural editing

(defconst vibelang--entity-regexp
  (rx symbol-start
      (? (seq "let"
              (+ space)
              (group (+ (or word ?_)))
              (* space)
              "="
              (* space)))
      (group (or "define_group" "voice" "pattern" "melody" "sequence"))
      (* space)
      "("
      (* space)
      (group "\""
             (* (not (any ?\")))
             "\""))
  "Regexp matching the start of a VibeLang entity definition.")

(defun vibelang--code-context-p (&optional pos)
  "Return non-nil when POS is not inside a string or comment."
  (let ((state (syntax-ppss pos)))
    (not (or (nth 3 state) (nth 4 state)))))

(defun vibelang--read-string-literal (literal)
  "Decode VibeLang string LITERAL into a plain Emacs string."
  (condition-case nil
      (read literal)
    (error (string-trim literal "\"" "\""))))

(defun vibelang--entity-type-from-constructor (constructor)
  "Map entity CONSTRUCTOR name to a semantic entity type symbol."
  (pcase constructor
    ("define_group" 'group)
    ("voice" 'voice)
    ("pattern" 'pattern)
    ("melody" 'melody)
    ("sequence" 'sequence)
    ("fx" 'fx)
    (_ (intern constructor))))

(defun vibelang--find-statement-end (start)
  "Return the end position of the entity statement starting at START."
  (save-excursion
    (goto-char start)
    (catch 'done
      (while (re-search-forward ";" nil t)
        (when (vibelang--code-context-p (match-beginning 0))
          (throw 'done (point))))
      (point-max))))

(defun vibelang--entity-from-match ()
  "Build an entity plist from the current match data."
  (let* ((var (match-string-no-properties 1))
         (constructor (match-string-no-properties 2))
         (name-literal (match-string-no-properties 3))
         (start (match-beginning 0))
         (name-start (match-beginning 3))
         (name-end (match-end 3))
         (var-start (match-beginning 1))
         (var-end (match-end 1))
         (end (vibelang--find-statement-end start)))
    (list :type (vibelang--entity-type-from-constructor constructor)
          :constructor constructor
          :name (vibelang--read-string-literal name-literal)
          :name-bounds (and name-start name-end
                            (cons (1+ name-start) (1- name-end)))
          :var var
          :var-bounds (and var var-start var-end (cons var-start var-end))
          :start start
          :end end)))

(defun vibelang--search-entity-forward (&optional bound)
  "Search forward for the next VibeLang entity definition before BOUND.
Return its plist, or nil when none are found. Point is left at the match end."
  (catch 'found
    (while (re-search-forward vibelang--entity-regexp bound t)
      (when (vibelang--code-context-p (match-beginning 0))
        (let ((entity (vibelang--entity-from-match))
              (next-pos (match-end 0)))
          (goto-char next-pos)
          (throw 'found entity))))))

(defun vibelang--search-entity-backward (&optional bound)
  "Search backward for the previous VibeLang entity definition after BOUND.
Return its plist, or nil when none are found. Point is left at the match start."
  (catch 'found
    (while (re-search-backward vibelang--entity-regexp bound t)
      (when (vibelang--code-context-p (match-beginning 0))
        (let ((entity (vibelang--entity-from-match))
              (prev-pos (match-beginning 0)))
          (goto-char prev-pos)
          (throw 'found entity))))))

(defun vibelang--statement-start (pos)
  "Return the start position of the statement containing POS."
  (save-excursion
    (goto-char pos)
    (if (re-search-backward ";" nil t)
        (progn
          (forward-char 1)
          (skip-chars-forward " \t\n")
          (point))
      (point-min))))

(defun vibelang--entity-at-point ()
  "Return the VibeLang entity containing point, if any."
  (save-excursion
    (let* ((pos (point))
           (stmt-start (vibelang--statement-start pos))
           (stmt-end (vibelang--find-statement-end stmt-start))
           entity)
      (goto-char stmt-start)
      (setq entity (vibelang--search-entity-forward stmt-end))
      (when (and entity
                 (<= (plist-get entity :start) pos)
                 (<= pos (plist-get entity :end)))
        entity))))

(defun vibelang--entity-before-point ()
  "Return the nearest VibeLang entity starting before point, if any."
  (save-excursion
    (let ((origin (point))
          entity
          previous)
      (goto-char (point-min))
      (while (and (setq entity (vibelang--search-entity-forward))
                  (< (plist-get entity :start) origin))
        (setq previous entity))
      previous)))

(defun vibelang--entity-after-point ()
  "Return the nearest VibeLang entity starting after point, if any."
  (save-excursion
    (let ((origin (point))
          entity)
      (goto-char (point-min))
      (while (and (setq entity (vibelang--search-entity-forward))
                  (<= (plist-get entity :start) origin))
        (goto-char (plist-get entity :end)))
      entity)))

(defun vibelang-current-entity ()
  "Echo the semantic VibeLang entity at point.
Returns the entity plist when called from Lisp."
  (interactive)
  (let ((entity (vibelang--entity-at-point)))
    (unless entity
      (user-error "No VibeLang entity at point"))
    (when (called-interactively-p 'interactive)
      (message "%s %s%s"
               (capitalize (symbol-name (plist-get entity :type)))
               (plist-get entity :name)
               (if-let ((var (plist-get entity :var)))
                   (format "  [let %s]" var)
                 "")))
    entity))

(defun vibelang-beginning-of-entity (&optional arg)
  "Move to the beginning of the current or previous VibeLang entity.
With ARG, move backward across that many entities."
  (interactive "p")
  (let ((arg (or arg 1)))
    (if (< arg 0)
        (vibelang-end-of-entity (- arg))
      (dotimes (_ arg)
        (let* ((origin (point))
               (current (vibelang--entity-at-point))
               (target (cond
                        ((and current (> origin (plist-get current :start))) current)
                        (t (vibelang--entity-before-point)))))
          (unless target
            (user-error "No previous VibeLang entity"))
          (goto-char (plist-get target :start)))))))

(defun vibelang-end-of-entity (&optional arg)
  "Move to the end of the current or next VibeLang entity.
With ARG, move forward across that many entities."
  (interactive "p")
  (let ((arg (or arg 1)))
    (if (< arg 0)
        (vibelang-beginning-of-entity (- arg))
      (dotimes (_ arg)
        (let ((target (or (vibelang--entity-at-point)
                          (vibelang--entity-after-point))))
          (unless target
            (user-error "No next VibeLang entity"))
          (goto-char (plist-get target :end)))))))

(defun vibelang-forward-entity (&optional arg)
  "Move to the start of the next VibeLang entity.
With ARG, repeat that many times. Negative ARG moves backward."
  (interactive "p")
  (let ((arg (or arg 1)))
    (if (< arg 0)
        (vibelang-backward-entity (- arg))
      (dotimes (_ arg)
        (let ((target (vibelang--entity-after-point)))
          (unless target
            (user-error "No next VibeLang entity"))
          (goto-char (plist-get target :start)))))))

(defun vibelang-backward-entity (&optional arg)
  "Move to the start of the previous VibeLang entity.
With ARG, repeat that many times. Negative ARG moves forward."
  (interactive "p")
  (let ((arg (or arg 1)))
    (if (< arg 0)
        (vibelang-forward-entity (- arg))
      (dotimes (_ arg)
        (let ((target (vibelang--entity-before-point)))
          (unless target
            (user-error "No previous VibeLang entity"))
          (goto-char (plist-get target :start)))))))

(defun vibelang-mark-entity ()
  "Mark the current VibeLang entity statement."
  (interactive)
  (let ((entity (vibelang-current-entity)))
    (goto-char (plist-get entity :start))
    (push-mark (plist-get entity :end) t t)
    (message "Marked %s %s"
             (symbol-name (plist-get entity :type))
             (plist-get entity :name))))

(defun vibelang--rename-entity-bounds (entity new-name &optional new-var)
  "Rewrite ENTITY metadata in place with NEW-NAME and optional NEW-VAR."
  (save-excursion
    (let ((edits nil))
      (when-let ((name-bounds (plist-get entity :name-bounds)))
        (push (list (car name-bounds) (cdr name-bounds) new-name) edits))
      (when-let ((var-bounds (plist-get entity :var-bounds)))
        (push (list (car var-bounds) (cdr var-bounds)
                    (or new-var (plist-get entity :var)))
              edits))
      (dolist (edit (sort edits (lambda (a b) (> (car a) (car b)))))
        (goto-char (nth 0 edit))
        (delete-region (nth 0 edit) (nth 1 edit))
        (insert (nth 2 edit))))))

(defun vibelang-rename-entity-definition (new-name new-var)
  "Rename the current entity definition to NEW-NAME.
When the entity is bound with `let', also rename the variable to NEW-VAR."
  (interactive
   (let* ((entity (vibelang-current-entity))
          (current-name (plist-get entity :name))
          (current-var (plist-get entity :var)))
     (list (read-string (format "Rename %s to: " current-name) nil nil current-name)
           (when current-var
             (read-string (format "Rename let binding %s to (blank keeps it): " current-var)
                          nil nil current-var)))))
  (let ((entity (vibelang-current-entity)))
    (vibelang--rename-entity-bounds entity new-name
                                    (and (plist-get entity :var)
                                         (not (string-empty-p new-var))
                                         new-var))
    (message "Renamed %s to %s"
             (symbol-name (plist-get entity :type))
             new-name)))

(defun vibelang-clone-entity (new-name new-var)
  "Clone the current VibeLang entity, giving the copy NEW-NAME and NEW-VAR."
  (interactive
   (let* ((entity (vibelang-current-entity))
          (current-name (plist-get entity :name))
          (current-var (plist-get entity :var))
          (default-var (and current-var
                            (concat current-var "_copy"))))
     (list (read-string (format "Clone %s as: " current-name)
                        nil nil (concat current-name "_copy"))
           (when current-var
             (read-string (format "New let binding (blank keeps %s_copy): " current-var)
                          nil nil default-var)))))
  (let* ((entity (vibelang-current-entity))
         (text (buffer-substring-no-properties (plist-get entity :start)
                                               (plist-get entity :end)))
         (insert-pos (plist-get entity :end))
         cloned)
    (save-excursion
      (goto-char insert-pos)
      (unless (bolp)
        (insert "\n"))
      (insert "\n" text)
      (setq cloned (save-excursion
                     (goto-char (1+ insert-pos))
                     (vibelang--search-entity-forward))))
    (unless cloned
      (user-error "Failed to locate cloned entity"))
    (vibelang--rename-entity-bounds cloned new-name
                                    (and (plist-get cloned :var)
                                         (if (or (null new-var)
                                                 (string-empty-p new-var))
                                             (concat (plist-get cloned :var) "_copy")
                                           new-var)))
    (goto-char (plist-get cloned :start))
    (message "Cloned %s %s as %s"
             (symbol-name (plist-get entity :type))
             (plist-get entity :name)
             new-name)))

(defun vibelang-current-defun-name ()
  "Return a semantic defun-like name for the entity at point."
  (when-let ((entity (vibelang--entity-at-point)))
    (format "%s %s"
            (symbol-name (plist-get entity :type))
            (plist-get entity :name))))

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
    (vibelang-cockpit-refresh)
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
    (accept-process-output vibelang--runtime-process 0.5)
    (when (process-live-p vibelang--runtime-process)
      (kill-process vibelang--runtime-process))
    (setq vibelang--api-ready nil)
    (vibelang-cockpit-refresh)
    (message "VibeLang runtime stopped")))

(defun vibelang--runtime-sentinel (_process event)
  "Handle VibeLang runtime EVENT."
  (let ((event (string-trim event)))
    (cond
     ((string-match-p "finished\\|exited" event)
      (message "VibeLang runtime exited: %s" event)
      (vibelang-ws-disconnect)
      (vibelang-cockpit-refresh))
     ((string-match-p "killed\\|terminated" event)
      (message "VibeLang runtime terminated")
      (vibelang-cockpit-refresh)))))

(defun vibelang-restart-runtime ()
  "Restart the VibeLang runtime with the current script."
  (interactive)
  (let ((script (buffer-file-name)))
    (if (and script (string-match-p "\\.vibe\\'" script))
        (progn
          (vibelang-stop-runtime)
          (run-at-time 0.5 nil #'vibelang-start-runtime script))
      (message "No .vibe file in current buffer"))))

(defun vibelang-runtime-running-p ()
  "Return non-nil if the VibeLang runtime is running."
  (and vibelang--runtime-process
       (process-live-p vibelang--runtime-process)))

(defun vibelang--on-after-save ()
  "Flash the whole buffer after save to confirm hot-reload was triggered."
  (when (derived-mode-p 'vibelang-mode)
    (vibelang--flash-region (point-min) (point-max))
    (when vibelang-auto-reload-on-save
      (vibelang-reload-script))))

(defun vibelang--save-current-buffer-if-needed ()
  "Save the current buffer when `vibelang-eval-save-before-run' allows it."
  (when (and vibelang-eval-save-before-run
             (derived-mode-p 'vibelang-mode)
             buffer-file-name
             (buffer-modified-p))
    (save-buffer)))

(defun vibelang--current-script-name ()
  "Return a human-friendly description of the current VibeLang buffer."
  (or (and buffer-file-name (abbreviate-file-name buffer-file-name))
      (buffer-name)))

(defun vibelang--active-script-buffer ()
  "Return a live VibeLang source buffer, preferring the current one."
  (or (and (derived-mode-p 'vibelang-mode) (current-buffer))
      (catch 'found
        (dolist (buf (buffer-list))
          (with-current-buffer buf
            (when (derived-mode-p 'vibelang-mode)
              (throw 'found buf)))))))

(defun vibelang--ensure-api-ready ()
  "Raise a user error when the VibeLang HTTP API is unavailable."
  (unless (vibelang-ws-api-ready-p)
    (user-error "VibeLang HTTP API is not responding at %s" (vibelang-ws-api-base-url))))

(defun vibelang--eval-text (code label)
  "Evaluate CODE via the VibeLang HTTP API using LABEL for feedback."
  (vibelang--ensure-api-ready)
  (let ((result (vibelang-ws-eval-string code label)))
    (when (get-buffer vibelang-cockpit-buffer-name)
      (vibelang-cockpit-refresh))
    (unless (plist-get result :success)
      (user-error "%s" (plist-get result :message)))
    result))

(defun vibelang-eval-dwim ()
  "Evaluate the active region, otherwise reload the whole buffer."
  (interactive)
  (if (use-region-p)
      (vibelang-eval-region (region-beginning) (region-end))
    (vibelang-eval-buffer)))

(defun vibelang-eval-region (beg end)
  "Evaluate the region from BEG to END as VibeLang code."
  (interactive "r")
  (unless (use-region-p)
    (user-error "No active region"))
  (vibelang--eval-text (buffer-substring-no-properties beg end)
                       (format "eval region (%s)" (buffer-name)))
  (vibelang--flash-region beg end))

(defun vibelang-eval-buffer ()
  "Evaluate the current buffer as VibeLang code."
  (interactive)
  (vibelang--save-current-buffer-if-needed)
  (vibelang--eval-text (buffer-substring-no-properties (point-min) (point-max))
                       (format "eval buffer (%s)" (vibelang--current-script-name)))
  (vibelang--flash-region (point-min) (point-max)))

(defun vibelang-eval-line ()
  "Evaluate the current line as VibeLang code."
  (interactive)
  (let ((code (string-trim (thing-at-point 'line t))))
    (when (string-empty-p code)
      (user-error "Current line is empty"))
    (vibelang--eval-text code
                         (format "eval line (%s:%d)"
                                 (buffer-name)
                                 (line-number-at-pos)))
    (vibelang--flash-region (line-beginning-position) (line-end-position))))

(defun vibelang-eval-prompt (code)
  "Prompt for a VibeLang CODE snippet and evaluate it."
  (interactive (list (read-string "VibeLang eval: ")))
  (when (string-empty-p (string-trim code))
    (user-error "No VibeLang code provided"))
  (vibelang--eval-text code "eval prompt"))

(defun vibelang-disconnect ()
  "Disconnect from VibeLang WebSocket server."
  (interactive)
  (vibelang-ws-disconnect))

(defun vibelang-transport-start ()
  "Start transport playback."
  (interactive)
  (vibelang--ensure-api-ready)
  (let ((result (vibelang-ws-send-command "transport.start")))
    (when (get-buffer vibelang-cockpit-buffer-name)
      (vibelang-cockpit-refresh))
    (unless (plist-get result :success)
      (user-error "%s" (plist-get result :message)))))

(defun vibelang-transport-stop ()
  "Stop transport playback."
  (interactive)
  (vibelang--ensure-api-ready)
  (let ((result (vibelang-ws-send-command "transport.stop")))
    (when (get-buffer vibelang-cockpit-buffer-name)
      (vibelang-cockpit-refresh))
    (unless (plist-get result :success)
      (user-error "%s" (plist-get result :message)))))

;;; Tap tempo

(defvar vibelang--tap-tempo-times nil
  "List of recent tap timestamps for tap-tempo calculation.")

(defvar vibelang--tap-tempo-timer nil
  "Timer to reset tap-tempo state after inactivity.")

(defcustom vibelang-tap-tempo-timeout 3.0
  "Seconds of inactivity before tap-tempo resets."
  :type 'float
  :group 'vibelang)

(defun vibelang--send-set-tempo (bpm)
  "Send BPM to the runtime via PATCH /transport."
  (let* ((response (vibelang--http-request "PATCH" "/transport" `((bpm . ,bpm))))
         (ok (plist-get response :ok)))
    (unless ok
      (message "VibeLang set-tempo failed: %s"
               (or (plist-get response :error) "unknown error")))))

(defun vibelang--current-bpm ()
  "Return the current BPM from playback state, or 120 if not connected."
  (or (and vibelang--playback-state
           (alist-get 'bpm (alist-get 'transport vibelang--playback-state)))
      120))

(defun vibelang-bpm-nudge-up (arg)
  "Increment BPM by ARG (default 1, prefix arg 5)."
  (interactive "P")
  (let ((step (if arg 5 1)))
    (vibelang--send-set-tempo (+ (vibelang--current-bpm) step))))

(defun vibelang-bpm-nudge-down (arg)
  "Decrement BPM by ARG (default 1, prefix arg 5)."
  (interactive "P")
  (let ((step (if arg 5 1)))
    (vibelang--send-set-tempo (- (vibelang--current-bpm) step))))

(defun vibelang-set-bpm (bpm)
  "Set transport BPM to BPM."
  (interactive "nBPM: ")
  (vibelang--send-set-tempo bpm))

(defun vibelang-tap-tempo ()
  "Tap to set BPM. Tap multiple times to average. Resets after inactivity."
  (interactive)
  (let ((now (float-time)))
    (when vibelang--tap-tempo-timer
      (cancel-timer vibelang--tap-tempo-timer))
    (setq vibelang--tap-tempo-timer
          (run-at-time vibelang-tap-tempo-timeout nil
                       (lambda () (setq vibelang--tap-tempo-times nil))))
    (push now vibelang--tap-tempo-times)
    (when (>= (length vibelang--tap-tempo-times) 2)
      (let* ((times (reverse vibelang--tap-tempo-times))
             (intervals (cl-mapcar #'- (cdr times) times))
             (avg-interval (/ (apply #'+ intervals) (length intervals)))
             (bpm (round (/ 60.0 avg-interval))))
        (message "VibeLang tap tempo: %d BPM" bpm)
        (vibelang--send-set-tempo bpm)))))

(defun vibelang-toggle-visualization ()
  "Toggle playback visualization on/off."
  (interactive)
  (vibelang-visualization-mode (if vibelang-visualization-mode -1 1))
  (message "Visualization %s" (if vibelang-visualization-mode "enabled" "disabled")))

(defun vibelang-reload-script ()
  "Reload the current script via the VibeLang HTTP eval endpoint."
  (interactive)
  (let ((file (buffer-file-name)))
    (unless file
      (user-error "Current buffer is not visiting a .vibe file"))
    (vibelang--save-current-buffer-if-needed)
    (vibelang--ensure-api-ready)
    (let ((result (vibelang-ws-send-command "eval.reload" `((file . ,file)))))
      (when (get-buffer vibelang-cockpit-buffer-name)
        (vibelang-cockpit-refresh))
      (unless (plist-get result :success)
        (user-error "%s" (plist-get result :message))))))

(defun vibelang-toggle-header-line ()
  "Toggle the transport header-line display."
  (interactive)
  (if header-line-format
      (setq-local header-line-format nil)
    (vibelang--setup-header-line))
  (message "Transport header %s" (if header-line-format "enabled" "disabled")))

(defun vibelang-cockpit-open ()
  "Open the VibeLang cockpit buffer."
  (interactive)
  (let ((buf (get-buffer-create vibelang-cockpit-buffer-name)))
    (with-current-buffer buf
      (unless (derived-mode-p 'vibelang-cockpit-mode)
        (vibelang-cockpit-mode))
      (vibelang-cockpit-refresh))
    (display-buffer buf '((display-buffer-reuse-window display-buffer-pop-up-window)
                          (window-height . 20)))))

(defun vibelang-cockpit-refresh ()
  "Refresh the VibeLang cockpit buffer."
  (interactive)
  (when-let ((buf (get-buffer vibelang-cockpit-buffer-name)))
    (with-current-buffer buf
      (let* ((inhibit-read-only t)
             (playback (vibelang-ws-playback-state))
             (transport (and playback (alist-get 'transport playback)))
             (command (vibelang-ws-last-command-result))
             (command-ok (plist-get command :success))
             (script-buf (vibelang--active-script-buffer)))
        (erase-buffer)
        (insert (propertize "VIBELANG COCKPIT\n" 'face 'bold))
        (insert (make-string 72 ?═) "\n\n")
        (insert (format "Script     %s\n"
                        (if script-buf
                            (with-current-buffer script-buf
                              (vibelang--current-script-name))
                          "(open a .vibe buffer for buffer-local actions)")))
        (insert (format "Runtime    %s\n"
                        (if (vibelang-runtime-running-p)
                            (format "running (%s)" (process-name vibelang--runtime-process))
                          "stopped")))
        (insert (format "WebSocket  %s\n" (vibelang-ws-status-string)))
        (insert (format "HTTP API   %s %s\n"
                        (vibelang-ws-api-base-url)
                        (if (vibelang-ws-api-ready-p) "(ready)" "(offline)")))
        (insert (format "Transport  %s\n"
                        (if transport
                            (format "%s bar %d.%d @ %.0f BPM"
                                    (if (eq (alist-get 'playing transport) t) "playing" "stopped")
                                    (1+ (or (alist-get 'bar transport) 0))
                                    (1+ (or (alist-get 'beat_in_bar transport) 0))
                                    (or (alist-get 'bpm transport) 120))
                          "no snapshot yet")))
        (insert "\n")
        (insert (propertize "Last action\n" 'face 'underline))
        (if command
            (progn
              (insert (propertize (format "%s — %s\n"
                                          (plist-get command :command)
                                          (plist-get command :message))
                                  'face (if command-ok 'success 'error)))
              (when-let ((preview (plist-get command :preview)))
                (insert (format "  %s\n" preview)))
              (insert (format "  %s\n"
                              (format-time-string "%Y-%m-%d %H:%M:%S"
                                                  (plist-get command :time)))))
          (insert "No commands yet.\n"))
        (insert "\n")
        (insert (propertize "Keys\n" 'face 'underline))
        (insert "g refresh   c connect   d disconnect   s start runtime   k stop runtime\n")
        (insert "R restart   r reload    e eval dwim    p play            x stop transport\n")
        (insert "b sidebar   ? diagnostics\n\n")
        (insert (propertize "Recent actions\n" 'face 'underline))
        (if-let ((log (vibelang-ws-command-log)))
            (dolist (entry log)
              (insert (format "%s  %-16s %s\n"
                              (format-time-string "%H:%M:%S" (plist-get entry :time))
                              (if (plist-get entry :success) "ok" "error")
                              (plist-get entry :message))))
          (insert "(empty)\n"))
        (goto-char (point-min))))))

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
         (ws-state (vibelang-ws-connection-state))
         (status (vibelang-ws-status-string)))
    (cond
     ((eq ws-state 'disconnected)
      (concat " " (propertize "DISCONNECTED" 'face 'vibelang-header-stopped-face)
              "  Press C-c C-c to connect"))
     ((or (eq ws-state 'connected)
          (eq ws-state 'reconnecting)
          (eq ws-state 'protocol-mismatch)
          (not transport))
      (concat " "
              (propertize (vibelang-ws-state-label ws-state)
                          'face (if (eq ws-state 'protocol-mismatch)
                                    'vibelang-header-stopped-face
                                  'font-lock-comment-face))
              "  "
              status))
     (t
      (let* ((playing (eq (alist-get 'playing transport) t))
             (bar (or (alist-get 'bar transport) 0))
             (beat-in-bar (or (alist-get 'beat_in_bar transport) 0))
             (bpm (or (alist-get 'bpm transport) 120))
             (time-sig (alist-get 'time_sig transport))
             (time-sig-parts (vibelang-ws-time-signature-components time-sig))
             (num (car time-sig-parts))
             (denom (cadr time-sig-parts))
             (transport-face (if (eq ws-state 'stale)
                                 'font-lock-warning-face
                               (if playing
                                   'vibelang-header-playing-face
                                 'vibelang-header-stopped-face))))
        (concat
         " "
         (propertize
          (cond
           ((eq ws-state 'stale) "⚠ STALE")
           (playing "▶ PLAYING")
           (t "■ STOPPED"))
          'face transport-face)
         "  │  "
         (propertize (format "Bar %d.%d" (1+ bar) (1+ beat-in-bar))
                     'face 'vibelang-header-beat-face)
         "  │  "
         (propertize (format "♩ = %.0f" bpm)
                     'face 'vibelang-header-bpm-face)
         "  │  "
         (format "%d/%d" num denom)
         "  │  "
         (vibelang--format-header-progress beat-in-bar num)
         (if (eq ws-state 'stale)
             (concat "  │  " status)
           "")))))))

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

(defun vibelang--on-playback-state-refresh (_state)
  "Refresh cockpit on playback state change."
  (vibelang-cockpit-refresh))

(defun vibelang--on-connection-state-update (_state _detail)
  "Update mode-line and cockpit on connection state change."
  (vibelang--update-mode-line (vibelang-ws-playback-state))
  (force-mode-line-update t)
  (vibelang-cockpit-refresh))

(defun vibelang--on-command-result-refresh (_result)
  "Refresh cockpit after a command result arrives."
  (vibelang-cockpit-refresh))

;; Hook into playback and connection state updates
(add-hook 'vibelang--playback-state-hook #'vibelang--update-header-transport)
(add-hook 'vibelang--playback-state-hook #'vibelang--on-playback-state-refresh)
(add-hook 'vibelang--connection-state-hook #'vibelang--on-connection-state-update)
(add-hook 'vibelang--command-result-hook #'vibelang--on-command-result-refresh)

;;; Mode-line indicator

(defvar vibelang--mode-line-string ""
  "Mode line string showing VibeLang playback state.")

(defun vibelang--update-mode-line (state)
  "Update mode line with playback STATE."
  (let ((ws-state (vibelang-ws-connection-state)))
    (cond
     (state
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
                (format " [%s %s]"
                        (if (eq ws-state 'stale)
                            (propertize "!" 'face 'font-lock-warning-face)
                          (propertize "||" 'face '(:foreground "red")))
                        (if (eq ws-state 'stale) "STALE" "STOPPED"))))))
     ((eq ws-state 'connected)
      (setq vibelang--mode-line-string " [WS CONNECTED]") )
     ((eq ws-state 'reconnecting)
      (setq vibelang--mode-line-string " [WS RECONNECTING]"))
     ((eq ws-state 'protocol-mismatch)
      (setq vibelang--mode-line-string " [WS MISMATCH]"))
     (t
      (setq vibelang--mode-line-string ""))))
  (force-mode-line-update t))

;; Add to global mode string
(unless (member '(:eval vibelang--mode-line-string) global-mode-string)
  (setq global-mode-string
        (append global-mode-string '((:eval vibelang--mode-line-string)))))

;;; Live parameter editing

(defun vibelang--send-entity-param (entity param value)
  "Send a live parameter update for ENTITY's PARAM to VALUE.
Returns non-nil on success."
  (let* ((name (plist-get entity :name))
         (type (plist-get entity :type))
         (result
          (pcase type
            ((or 'voice 'group 'fx 'modulator)
             (let ((path (pcase type
                           ('voice     (format "/voices/%s/params/%s"     name param))
                           ('group     (format "/groups/%s/params/%s"     name param))
                           ('fx        (format "/effects/%s/params/%s"    name param))
                           ('modulator (format "/modulators/%s/params/%s" name param)))))
               (vibelang--http-request "PUT" path `((value . ,value)))))
            ('pattern
             (vibelang--http-request
              "PATCH" (format "/patterns/%s" name)
              `((params . ((,(intern param) . ,value))))))
            (_  nil))))
    (and result (plist-get result :ok))))

(defun vibelang-tweak-param-at-point ()
  "Interactively tweak a parameter at point using arrow keys.
Prompts for parameter name (pre-filled from context), then enters a
transient mode: ↑/↓ nudge the value live, ←/→ change step size,
any other key exits."
  (interactive)
  (let* ((param  (read-string "Parameter: " (vibelang--param-name-at-point)))
         (entity (vibelang--entity-at-point)))
    (when (string-empty-p param)
      (user-error "No parameter name"))
    (unless entity
      (user-error "No VibeLang entity at point"))
    (let ((value (or (vibelang--number-at-point) 0.0))
          (step  0.01)
          (ename (plist-get entity :name)))
      (cl-flet
          ((show ()
             (message "[%s.%s = %.4g]  ↑↓ nudge  ←→ step (%.4g)  other key: done"
                      ename param value step))
           (nudge (delta)
             (setq value (+ value delta))
             (vibelang--send-entity-param entity param value)
             (message "[%s.%s = %.4g]  ↑↓ nudge  ←→ step (%.4g)  other key: done"
                      ename param value step)))
        (show)
        (set-transient-map
         (let ((map (make-sparse-keymap)))
           (define-key map (kbd "<up>")
             (lambda () (interactive) (nudge step)))
           (define-key map (kbd "<down>")
             (lambda () (interactive) (nudge (- step))))
           (define-key map (kbd "<right>")
             (lambda () (interactive)
               (setq step (* step 10))
               (show)))
           (define-key map (kbd "<left>")
             (lambda () (interactive)
               (setq step (/ step 10.0))
               (show)))
           map)
         t
         (lambda ()
           (message "VibeLang: %s.%s set to %.4g" ename param value)))))))

(defun vibelang--number-at-point ()
  "Return the number at or near point, or nil."
  (or (thing-at-point 'number t)
      (save-excursion
        (skip-chars-backward "0-9.")
        (thing-at-point 'number t))))

(defun vibelang--param-name-at-point ()
  "Return the method name from a call like .NAME( searching backward from point."
  (save-excursion
    (when (re-search-backward "\\.\\([a-zA-Z_][a-zA-Z0-9_]*\\)\\s-*(" nil t)
      (match-string-no-properties 1))))

(defun vibelang-edit-param-at-point (param new-value)
  "Edit parameter PARAM on the entity at point and send a live update to the runtime.
Detects the parameter name from the enclosing method call (e.g. .volume() -> volume)
and uses the number at/near point as the default value."
  (interactive
   (list (read-string "Parameter: " (vibelang--param-name-at-point))
         (read-number "Value: " (vibelang--number-at-point))))
  (when (string-empty-p param)
    (user-error "No parameter name"))
  (let ((entity (vibelang--entity-at-point)))
    (unless entity
      (user-error "No VibeLang entity at point"))
    (let* ((name (plist-get entity :name))
           (type (plist-get entity :type)))
      (pcase type
        ((or 'melody 'sequence)
         (user-error "%s '%s' has no live param endpoint — edit source and save to reload" type name))
        ('recording
         (user-error "recording '%s' has no live param interface" name))
        (_
         (if (vibelang--send-entity-param entity param new-value)
             (progn
               (vibelang--flash-region (plist-get entity :start) (plist-get entity :end))
               (message "VibeLang: set %s.%s = %s" name param new-value))
           (message "VibeLang: failed to set %s.%s (unsupported type: %s)" name param type)))))))

;;; Mute / solo from buffer

(defun vibelang--mute-entity (entity muted)
  "Send a mute or unmute request for ENTITY.
MUTED non-nil sends mute, nil sends unmute."
  (let* ((name (plist-get entity :name))
         (type (plist-get entity :type))
         (action (if muted "mute" "unmute"))
         (path (pcase type
                 ('voice  (format "/voices/%s/%s"  name action))
                 ('group  (format "/groups/%s/%s"  name action))
                 (_       nil))))
    (if (not path)
        (message "VibeLang: mute not supported for entity type %s" type)
      (let ((result (vibelang--http-request "POST" path)))
        (if (plist-get result :ok)
            (message "VibeLang: %s %s" (if muted "muted" "unmuted") name)
          (message "VibeLang: failed to %s %s: %s"
                   action name (or (plist-get result :error) "unknown error")))))))

(defun vibelang-mute-at-point ()
  "Mute the voice or group at or near point."
  (interactive)
  (if-let ((entity (vibelang--entity-at-point)))
      (vibelang--mute-entity entity t)
    (message "VibeLang: no entity at point")))

(defun vibelang-solo-at-point ()
  "Solo the group at or near point."
  (interactive)
  (if-let ((entity (vibelang--entity-at-point)))
      (let* ((name (plist-get entity :name))
             (type (plist-get entity :type))
             (path (pcase type
                     ('group (format "/groups/%s/solo" name))
                     (_      nil))))
        (if (not path)
            (message "VibeLang: solo is only supported for groups (got %s)" type)
          (let ((result (vibelang--http-request "POST" path)))
            (if (plist-get result :ok)
                (message "VibeLang: soloed %s" name)
              (message "VibeLang: failed to solo %s: %s"
                       name (or (plist-get result :error) "unknown error"))))))
    (message "VibeLang: no entity at point")))

;;; imenu integration

(defun vibelang-imenu-create-index ()
  "Build imenu index of VibeLang entities in current buffer."
  (let ((index '()))
    (save-excursion
      (goto-char (point-min))
      (while (re-search-forward
              "\\blet\\s-+\\(\\w+\\)\\s-*=\\s-*\\(voice\\|group\\|pattern\\|melody\\|sequence\\|fx\\|modulator\\|midi_device\\)\\s-*("
              nil t)
        (let* ((name (match-string-no-properties 1))
               (type (match-string-no-properties 2))
               (pos (match-beginning 0))
               (category (assoc type index)))
          (if category
              (setcdr category (cons (cons name (copy-marker pos)) (cdr category)))
            (push (list type (cons name (copy-marker pos))) index)))))
    (nreverse index)))

;;; describe-entity live buffer

(defvar-local vibelang--describe-entity-entity nil
  "The VibeLang entity plist for this describe-entity buffer.")

(defun vibelang--describe-entity-render (buf entity data)
  "Render ENTITY DATA into BUF."
  (with-current-buffer buf
    (let ((inhibit-read-only t))
      (erase-buffer)
      (let* ((name (plist-get entity :name))
             (type (capitalize (symbol-name (plist-get entity :type)))))
        (insert (format "%s: %s\n" type name))
        (if (not data)
            (insert "  (no data from API)\n")
          (dolist (pair data)
            (let ((val (cdr pair)))
              (insert (format "  %-12s %s\n"
                              (concat (symbol-name (car pair)) ":")
                              (cond
                               ((listp val)
                                (format "[%s]" (mapconcat #'prin1-to-string val " ")))
                               ((vectorp val)
                                (format "[%s]"
                                        (mapconcat #'prin1-to-string
                                                   (append val nil) " ")))
                               (t (format "%s" val))))))))))))

(defun vibelang--describe-entity-refresh (_state)
  "Auto-refresh live describe-entity buffers on playback state change."
  (dolist (buf (buffer-list))
    (when (and (buffer-live-p buf)
               (string-prefix-p "*vibelang-entity: " (buffer-name buf)))
      (with-current-buffer buf
        (when vibelang--describe-entity-entity
          (let* ((entity vibelang--describe-entity-entity)
                 (name (plist-get entity :name))
                 (type (plist-get entity :type))
                 (path (pcase type
                         ('voice (format "/voices/%s" name))
                         ('group (format "/groups/%s" name))
                         (_ nil))))
            (when path
              (let* ((result (vibelang--http-request "GET" path))
                     (data (plist-get result :json)))
                (vibelang--describe-entity-render buf entity data)))))))))

(defun vibelang-describe-entity ()
  "Show a live description buffer for the VibeLang entity at point."
  (interactive)
  (let ((entity (vibelang--entity-at-point)))
    (unless entity
      (user-error "No VibeLang entity at point"))
    (let* ((name (plist-get entity :name))
           (type (plist-get entity :type))
           (path (pcase type
                   ('voice     (format "/voices/%s"    name))
                   ('group     (format "/groups/%s"    name))
                   ('fx        (format "/effects/%s"   name))
                   ('modulator (format "/modulators/%s" name))
                   (_ (user-error "describe-entity not supported for %s entities" type)))))
      (let* ((result (vibelang--http-request "GET" path))
             (data (plist-get result :json))
             (buf-name (format "*vibelang-entity: %s*" name))
             (buf (get-buffer-create buf-name)))
        (with-current-buffer buf
          (unless (derived-mode-p 'special-mode)
            (special-mode))
          (setq vibelang--describe-entity-entity entity)
          (vibelang--describe-entity-render buf entity data)
          (goto-char (point-min)))
        (add-hook 'vibelang--playback-state-hook #'vibelang--describe-entity-refresh)
        (display-buffer buf '((display-buffer-reuse-window display-buffer-pop-up-window)))))))

;;; Help buffer

(defun vibelang--help-insert-heading (text)
  "Insert TEXT as a bold, coloured section heading."
  (insert (propertize (concat "\n" text "\n")
                      'face '(:weight bold :foreground "cyan" :height 1.1)))
  (insert (propertize (make-string (length text) ?-)
                      'face '(:foreground "cyan")))
  (insert "\n"))

(defun vibelang-help ()
  "Open the *VibeLang Help* buffer with a usage guide."
  (interactive)
  (let ((buf (get-buffer-create "*VibeLang Help*")))
    (with-current-buffer buf
      (let ((inhibit-read-only t))
        (erase-buffer)
        (special-mode)
        (insert (propertize "VibeLang Mode — Usage Guide\n" 'face '(:weight bold :height 1.4)))
        (insert (propertize (format "Mode version: %s  |  C-c C-. opens the full transient menu\n"
                                    (or (bound-and-true-p vibelang-version) "dev"))
                            'face 'shadow))

        ;; 1. Getting Started
        (vibelang--help-insert-heading "1. Getting Started")
        (insert "  Open any .vibe file — the mode activates automatically.\n")
        (insert "  By default, vibelang-mode connects to a running runtime on\n")
        (insert "  127.0.0.1:1606 if one is already listening (auto-connect: if-running).\n\n")
        (insert (format "  %-20s %s\n" "C-c C-c" "Connect to the VibeLang runtime"))
        (insert (format "  %-20s %s\n" "C-c C-d" "Disconnect"))
        (insert (format "  %-20s %s\n" "C-c r s" "Start the runtime (runs `vibe run <file>')"))
        (insert (format "  %-20s %s\n" "C-c r k" "Kill the runtime"))
        (insert (format "  %-20s %s\n" "C-c r r" "Restart the runtime"))

        ;; 2. Transport
        (vibelang--help-insert-heading "2. Transport")
        (insert (format "  %-20s %s\n" "C-c C-s" "Start playback"))
        (insert (format "  %-20s %s\n" "C-c C-x" "Stop playback"))
        (insert (format "  %-20s %s\n" "C-c C-t" "Tap tempo (tap 3+ times, auto-resets after timeout)"))
        (insert (format "  %-20s %s\n" "C-c +"   "Nudge BPM up by 1"))
        (insert (format "  %-20s %s\n" "C-c -"   "Nudge BPM down by 1"))
        (insert (format "  %-20s %s\n" "C-c C-q" "Set BPM to exact value (prompts)"))

        ;; 3. Entity Commands
        (vibelang--help-insert-heading "3. Entity Commands")
        (insert "  These act on the VibeLang entity at point (voice, group, pattern, etc.).\n\n")
        (insert (format "  %-20s %s\n" "C-c m"   "Mute entity at point"))
        (insert (format "  %-20s %s\n" "C-c s"   "Solo entity at point"))
        (insert (format "  %-20s %s\n" "C-c C-i" "Inspect entity (live description buffer)"))
        (insert (format "  %-20s %s\n" "C-c C-p" "Edit numeric param at point interactively"))
        (insert (format "  %-20s %s\n" "C-c n r" "Rename entity definition"))
        (insert (format "  %-20s %s\n" "C-c n c" "Clone entity definition"))
        (insert (format "  %-20s %s\n" "C-c n ." "Show name of entity at point in minibuffer"))

        ;; 4. Navigation
        (vibelang--help-insert-heading "4. Navigation")
        (insert (format "  %-20s %s\n" "M-n"     "Move to next entity definition"))
        (insert (format "  %-20s %s\n" "M-p"     "Move to previous entity definition"))
        (insert (format "  %-20s %s\n" "C-M-h"   "Mark (select) current entity"))
        (insert (format "  %-20s %s\n" "C-c /"   "Jump to entity by name (imenu)"))
        (insert (format "  %-20s %s\n" "C-c C-b" "Toggle sidebar"))
        (insert (format "  %-20s %s\n" "C-c b o" "Open sidebar"))
        (insert (format "  %-20s %s\n" "C-c b i" "Focus sidebar on entity at point"))

        ;; 5. Eval & Reload
        (vibelang--help-insert-heading "5. Eval & Reload")
        (insert (format "  %-20s %s\n" "C-c C-e" "Eval dwim: region, entity at point, or buffer"))
        (insert (format "  %-20s %s\n" "C-c e b" "Eval entire buffer"))
        (insert (format "  %-20s %s\n" "C-c e r" "Eval active region"))
        (insert (format "  %-20s %s\n" "C-c e l" "Eval current line"))
        (insert (format "  %-20s %s\n" "C-c e p" "Eval expression entered at prompt"))
        (insert (format "  %-20s %s\n" "C-c C-r" "Reload script from disk"))
        (insert "\n  Toggle auto-reload on save from C-c C-. > Session > Auto-reload.\n")

        ;; 6. Visualization
        (vibelang--help-insert-heading "6. Visualization")
        (insert "  With a connected runtime, VibeLang overlays beat position indicators\n")
        (insert "  inside pattern/melody blocks and highlights active clips in sequences.\n\n")
        (insert (format "  %-20s %s\n" "C-c C-v" "Toggle beat-position visualization"))
        (insert (format "  %-20s %s\n" "C-c C-b" "Toggle sidebar (shows live meters, clip progress)"))
        (insert "  The header line shows transport state: bar.beat @ BPM.\n")
        (insert (format "  %-20s %s\n" "C-c h t" "Toggle the transport header line"))

        ;; 7. LSP
        (vibelang--help-insert-heading "7. LSP")
        (insert "  Requires `vibelang lsp' (eglot, Emacs 29+) or lsp-mode.\n\n")
        (insert (format "  %-20s %s\n" "C-c C-l" "Enable LSP for this buffer"))
        (insert (format "  %-20s %s\n" "C-c l r" "Restart the LSP server"))
        (insert "\n  LSP provides: completion, hover docs, go-to-definition, rename,\n")
        (insert "  diagnostics, semantic token highlighting, and inlay hints.\n")
        (insert "  Error overlays appear inline; set vibelang-enable-lsp to t to enable\n")
        (insert "  LSP automatically when opening .vibe files.\n")

        ;; 8. Templates
        (vibelang--help-insert-heading "8. Templates")
        (insert "  Type a snippet key and expand with your template engine (tempel/yasnippet).\n\n")
        (insert (format "  %-8s %s\n" "voi"  "voice()  — synth + volume scaffold"))
        (insert (format "  %-8s %s\n" "grp"  "group()  — group with a voice add"))
        (insert (format "  %-8s %s\n" "pat"  "pattern() — step string"))
        (insert (format "  %-8s %s\n" "mel"  "melody()  — notes array"))
        (insert (format "  %-8s %s\n" "fx"   "fx()      — effect synth"))
        (insert (format "  %-8s %s\n" "loop" "midi_device().looper() scaffold"))
        (insert (format "  %-8s %s\n" "cc"   "midi_device().map_cc() scaffold"))
        (insert "\n  Call (vibelang-setup-templates) in your mode hook to activate.\n")

        ;; 9. Transient Menu
        (vibelang--help-insert-heading "9. Transient Menu")
        (insert (format "  %-20s %s\n" "C-c C-." "Open the full transient command menu"))
        (insert "\n  The menu groups all commands into: Transport, Entity, Session, Navigate.\n")
        (insert "  Useful when you don't remember a keybinding.\n")

        ;; 10. Troubleshooting
        (vibelang--help-insert-heading "10. Troubleshooting")
        (insert (format "  %-20s %s\n" "C-c C-?" "Run diagnostics (connection, API, state)"))
        (insert "\n  Common issues:\n")
        (insert "  - [WS MISMATCH]  — runtime and mode protocol versions differ.\n")
        (insert "                     Upgrade both `vibe' and vibelang-mode.\n")
        (insert "  - [WS STALE]     — runtime is running but not sending ticks.\n")
        (insert "                     Try C-c C-c to reconnect.\n")
        (insert "  - Not connecting — check vibelang-ws-host / vibelang-ws-port match\n")
        (insert "                     the port shown by `vibe run'.\n")
        (insert "  - Runtime log    — see *vibelang-runtime* buffer for stderr output.\n")

        (insert "\n")
        (goto-char (point-min))))
    (display-buffer buf '((display-buffer-reuse-window display-buffer-pop-up-window)))))

(provide 'vibelang-mode)
;;; vibelang-mode.el ends here
