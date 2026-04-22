;;; vibelang-indent.el --- Indentation for VibeLang -*- lexical-binding: t -*-

;; Copyright (C) 2024 VibeLang Authors
;; Author: VibeLang Team
;; Keywords: languages, music

;;; Commentary:
;; Provides indentation support for VibeLang (.vibe) files.
;; VibeLang uses Rhai syntax -- C-like with curly braces and semicolons.

;;; Code:

(defcustom vibelang-indent-offset 2
  "Number of spaces per indentation level in VibeLang mode."
  :type 'integer
  :group 'vibelang)

(defun vibelang--calculate-indent ()
  "Calculate the indentation column for the current line.

Uses `syntax-ppss' to determine brace/paren nesting depth.
Returns nil when point is inside a string or comment."
  (save-excursion
    (back-to-indentation)
    (let* ((ppss (syntax-ppss))
           (in-string (nth 3 ppss))
           (in-comment (nth 4 ppss)))
      (unless (or in-string in-comment)
        (let* ((depth (nth 0 ppss))
               (base (* depth vibelang-indent-offset)))
          (cond
           ;; Closing brace: dedent by one level
           ((eq (char-after) ?})
            (max 0 (- base vibelang-indent-offset)))
           ;; Method chain continuation: extra indent
           ((eq (char-after) ?.)
            (+ base vibelang-indent-offset))
           (t base)))))))

(defun vibelang-indent-line ()
  "Indent the current line according to VibeLang indentation rules."
  (interactive)
  (let ((indent (vibelang--calculate-indent)))
    (when indent
      (save-excursion
        (back-to-indentation)
        (let ((cur (current-column)))
          (unless (= cur indent)
            (delete-horizontal-space)
            (indent-to indent))))
      (when (< (current-column) indent)
        (back-to-indentation)))))

(provide 'vibelang-indent)
;;; vibelang-indent.el ends here
