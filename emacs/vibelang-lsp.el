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

;;; Eglot Integration

(defun vibelang-lsp--eglot-setup ()
  "Set up eglot for VibeLang."
  (when (and (featurep 'eglot) vibelang-lsp-use-eglot)
    ;; Register VibeLang server
    (add-to-list 'eglot-server-programs
                 `(vibelang-mode . ,vibelang-lsp-server-command))

    ;; Map semantic token types to faces
    ;; VibeLang LSP token types (from semantic_tokens.rs):
    ;; 0: namespace (groups)
    ;; 1: type (synthdefs)
    ;; 2: class (effects)
    ;; 3: function (voices)
    ;; 4: method
    ;; 5: property (parameters)
    ;; 6: variable
    ;; 7: string
    ;; 8: number
    ;; 9: keyword
    ;; 10: operator
    ;; 11: comment
    ;; 12: macro (patterns/melodies)
    ;; 13: parameter
    ;; 14: enumMember (note names)
    (when (boundp 'eglot-semantic-token-faces)
      (setq-local eglot-semantic-token-faces
                  '((namespace . vibelang-lsp-group-face)
                    (type . vibelang-lsp-synthdef-face)
                    (class . vibelang-lsp-synthdef-face)
                    (function . vibelang-lsp-voice-face)
                    (method . font-lock-function-name-face)
                    (property . font-lock-variable-name-face)
                    (variable . font-lock-variable-name-face)
                    (string . font-lock-string-face)
                    (number . font-lock-constant-face)
                    (keyword . font-lock-keyword-face)
                    (operator . font-lock-operator-face)
                    (comment . font-lock-comment-face)
                    (macro . vibelang-lsp-pattern-face)
                    (parameter . font-lock-variable-name-face)
                    (enumMember . vibelang-lsp-note-face))))))

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
    (message "VibeLang LSP enabled via eglot"))
   ((featurep 'lsp-mode)
    (vibelang-lsp--lsp-mode-setup)
    (vibelang-lsp--lsp-mode-ensure)
    (message "VibeLang LSP enabled via lsp-mode"))
   (t
    (message "No LSP client available. Install eglot (Emacs 29+) or lsp-mode."))))

(defun vibelang-lsp-disable ()
  "Disable LSP support for VibeLang in current buffer."
  (interactive)
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
