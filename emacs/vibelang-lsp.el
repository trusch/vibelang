;;; vibelang-lsp.el --- LSP integration for VibeLang -*- lexical-binding: t -*-

;; Copyright (C) 2024 VibeLang Authors
;; Author: VibeLang Team
;; Keywords: languages, lsp

;;; Commentary:
;; Provides LSP integration for VibeLang using eglot (Emacs 29+) or lsp-mode.
;; The VibeLang LSP server provides:
;; - Code completion with snippets
;; - Hover documentation
;; - Go-to-definition
;; - Find references
;; - Rename symbol
;; - Diagnostics
;; - Signature help
;; - Document symbols (outline)
;; - Semantic tokens (enhanced syntax highlighting)
;; - Inlay hints (parameter names, timing info)
;; - Code actions (quick fixes)

;;; Code:

(require 'cl-lib)

(defgroup vibelang-lsp nil
  "VibeLang LSP configuration."
  :group 'vibelang
  :prefix "vibelang-lsp-")

(defcustom vibelang-lsp-server-command '("vibelang" "lsp")
  "Command to start the VibeLang LSP server."
  :type '(repeat string)
  :group 'vibelang-lsp)

(defcustom vibelang-lsp-use-eglot t
  "Whether to use eglot for LSP (vs lsp-mode).
Eglot is built into Emacs 29+."
  :type 'boolean
  :group 'vibelang-lsp)

;;; Semantic Token Faces
;; VibeLang LSP provides custom semantic token types

(defface vibelang-lsp-synthdef-face
  '((t :inherit font-lock-type-face :weight bold))
  "Face for synthdef names in VibeLang."
  :group 'vibelang-lsp)

(defface vibelang-lsp-voice-face
  '((t :inherit font-lock-function-name-face))
  "Face for voice names in VibeLang."
  :group 'vibelang-lsp)

(defface vibelang-lsp-pattern-face
  '((t :inherit font-lock-variable-name-face :slant italic))
  "Face for pattern names in VibeLang."
  :group 'vibelang-lsp)

(defface vibelang-lsp-melody-face
  '((t :inherit font-lock-variable-name-face :slant italic))
  "Face for melody names in VibeLang."
  :group 'vibelang-lsp)

(defface vibelang-lsp-group-face
  '((t :inherit font-lock-constant-face))
  "Face for group names in VibeLang."
  :group 'vibelang-lsp)

(defface vibelang-lsp-note-face
  '((((class color) (background dark))
     :foreground "#98c379" :weight bold)
    (((class color) (background light))
     :foreground "#50a14f" :weight bold))
  "Face for note names (C4, D#5, etc.) in VibeLang."
  :group 'vibelang-lsp)

(defface vibelang-lsp-pattern-token-face
  '((((class color) (background dark))
     :foreground "#e5c07b")
    (((class color) (background light))
     :foreground "#c18401"))
  "Face for pattern tokens (x, X, -, .) in VibeLang."
  :group 'vibelang-lsp)

;;; Diagnostic Overlays

(defface vibelang-error-overlay-face
  '((t :inherit compilation-error :extend nil))
  "Face for inline error overlays."
  :group 'vibelang)

(defface vibelang-warning-overlay-face
  '((t :inherit compilation-warning :extend nil))
  "Face for inline warning overlays."
  :group 'vibelang)

(defun vibelang--clear-diagnostic-overlays ()
  "Remove all VibeLang diagnostic overlays from current buffer."
  (remove-overlays (point-min) (point-max) 'vibelang-diagnostic-overlay t))

(defun vibelang--show-diagnostic-overlays ()
  "Show flymake/eglot diagnostics as inline overlays after each error line."
  (vibelang--clear-diagnostic-overlays)
  (when (fboundp 'flymake-diagnostics)
    (dolist (diag (flymake-diagnostics))
      (let* ((beg (flymake-diagnostic-beg diag))
             (text (flymake-diagnostic-text diag))
             (type (flymake-diagnostic-type diag))
             (face (if (eq type :error)
                       'vibelang-error-overlay-face
                     'vibelang-warning-overlay-face))
             (line-end (save-excursion (goto-char beg) (line-end-position)))
             (ov (make-overlay line-end line-end)))
        (overlay-put ov 'vibelang-diagnostic-overlay t)
        (overlay-put ov 'after-string
                     (propertize (concat "  ← " text) 'face face))))))

(defun vibelang-lsp--setup-diagnostic-overlays ()
  "Hook diagnostic overlays into flymake for the current buffer."
  (add-hook 'flymake-after-update-hook #'vibelang--show-diagnostic-overlays nil t))

(defun vibelang-lsp--teardown-diagnostic-overlays ()
  "Remove diagnostic overlay hooks and clear overlays from current buffer."
  (remove-hook 'flymake-after-update-hook #'vibelang--show-diagnostic-overlays t)
  (vibelang--clear-diagnostic-overlays))

;;; Eglot Integration

(defun vibelang-lsp--eglot-setup ()
  "Set up eglot for VibeLang."
  (when (and (featurep 'eglot) vibelang-lsp-use-eglot)
    ;; Register VibeLang server
    (add-to-list 'eglot-server-programs
                 `(vibelang-mode . ,vibelang-lsp-server-command))

    ;; Note: eglot does not expose a semantic token face mapping variable.
    ;; The faces defined in this file (vibelang-lsp-*-face) are available for
    ;; future use if eglot gains such an API or via a custom eglot extension.
    ))

(defun vibelang-lsp--eglot-ensure ()
  "Ensure eglot is running for current buffer."
  (when (and (featurep 'eglot)
             vibelang-lsp-use-eglot
             (derived-mode-p 'vibelang-mode))
    (eglot-ensure)))

;;; lsp-mode Integration (alternative to eglot)

(defun vibelang-lsp--lsp-mode-setup ()
  "Set up lsp-mode for VibeLang."
  (when (and (featurep 'lsp-mode) (not vibelang-lsp-use-eglot))
    ;; Register VibeLang client
    (eval-after-load 'lsp-mode
      '(progn
         (lsp-register-client
          (make-lsp-client
           :new-connection (lsp-stdio-connection vibelang-lsp-server-command)
           :major-modes '(vibelang-mode)
           :server-id 'vibelang-lsp
           :priority -1))

         ;; Semantic token mappings for lsp-mode
         (setq lsp-semantic-token-faces
               '(("namespace" . vibelang-lsp-group-face)
                 ("type" . vibelang-lsp-synthdef-face)
                 ("class" . vibelang-lsp-synthdef-face)
                 ("function" . vibelang-lsp-voice-face)
                 ("macro" . vibelang-lsp-pattern-face)
                 ("enumMember" . vibelang-lsp-note-face)))))))

(defun vibelang-lsp--lsp-mode-ensure ()
  "Ensure lsp-mode is running for current buffer."
  (when (and (featurep 'lsp-mode)
             (not vibelang-lsp-use-eglot)
             (derived-mode-p 'vibelang-mode))
    (lsp)))

;;; Public API

(defun vibelang-lsp-enable ()
  "Enable LSP support for VibeLang in current buffer."
  (interactive)
  (cond
   ((and (featurep 'eglot) vibelang-lsp-use-eglot)
    (vibelang-lsp--eglot-setup)
    (vibelang-lsp--eglot-ensure)
    (vibelang-lsp--configure-inlay-hints)
    (vibelang-lsp--setup-diagnostic-overlays)
    (message "VibeLang LSP enabled via eglot"))
   ((featurep 'lsp-mode)
    (vibelang-lsp--lsp-mode-setup)
    (vibelang-lsp--lsp-mode-ensure)
    (vibelang-lsp--setup-diagnostic-overlays)
    (message "VibeLang LSP enabled via lsp-mode"))
   (t
    (message "No LSP client available. Install eglot (Emacs 29+) or lsp-mode."))))

(defun vibelang-lsp-disable ()
  "Disable LSP support for VibeLang in current buffer."
  (interactive)
  (vibelang-lsp--teardown-diagnostic-overlays)
  (cond
   ((and (featurep 'eglot) (eglot-managed-p))
    (eglot-shutdown (eglot-current-server)))
   ((and (featurep 'lsp-mode) (lsp-workspaces))
    (lsp-workspace-shutdown (car (lsp-workspaces)))))
  (message "VibeLang LSP disabled"))

(defun vibelang-lsp-restart ()
  "Restart the VibeLang LSP server."
  (interactive)
  (vibelang-lsp-disable)
  (sit-for 0.5)
  (vibelang-lsp-enable))

;;; Inlay Hints Configuration

(defun vibelang-lsp--configure-inlay-hints ()
  "Configure inlay hints for VibeLang.
VibeLang LSP provides:
- Parameter name hints
- Timing hints (e.g., ms values based on tempo)
- Note frequency hints
- Duration hints"
  (when (featurep 'eglot)
    ;; Enable inlay hints if available (Emacs 29.1+)
    (when (fboundp 'eglot-inlay-hints-mode)
      (eglot-inlay-hints-mode 1))))

;;; Initialization

(defun vibelang-lsp-init ()
  "Initialize LSP support for VibeLang mode."
  ;; Set up based on available client
  (cond
   ((and (featurep 'eglot) vibelang-lsp-use-eglot)
    (vibelang-lsp--eglot-setup))
   ((featurep 'lsp-mode)
    (vibelang-lsp--lsp-mode-setup))))

;; Auto-initialize when loading
(with-eval-after-load 'eglot
  (vibelang-lsp--eglot-setup))

(with-eval-after-load 'lsp-mode
  (vibelang-lsp--lsp-mode-setup))

(provide 'vibelang-lsp)
;;; vibelang-lsp.el ends here
