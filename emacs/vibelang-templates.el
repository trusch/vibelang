;;; vibelang-templates.el --- Snippet templates for VibeLang mode -*- lexical-binding: t -*-

;;; Commentary:
;; Provides snippet/template support for vibelang-mode via tempel (preferred)
;; or yasnippet (fallback).  Call `vibelang-setup-templates' from the mode hook.

;;; Code:

(defvar vibelang-tempel-templates
  '(vibelang-mode
    ("voi" "let " (p "name" name) " = voice()\n"
     "    .synth(\"" (p "synth") "\")\n"
     "    .volume(" (p "1.0" vol) ");\n")
    ("grp" "let " (p "name" name) " = group()\n"
     "    .add(" (p "voice_name") ");\n")
    ("pat" "let " (p "name" name) " = pattern(\"" (p "pattern") "\");\n")
    ("mel" "let " (p "name" name) " = melody([\n"
     "    \"" (p "C4") "\",\n"
     "]);\n")
    ("fx"  "let " (p "name" name) " = fx()\n"
     "    .synth(\"" (p "reverb") "\");\n")
    ("loop" "midi_device(\"" (p "device") "\")\n"
     "    .looper()\n"
     "    .to(" (p "voice_name") ");\n")
    ("cc"  "midi_device(\"" (p "device") "\")\n"
     "    .map_cc(" (p "14" cc) ")\n"
     "    .to(" (p "group_or_voice") ", \"" (p "amp") "\", 0.0, 1.0);\n"))
  "VibeLang tempel templates.")

(defun vibelang-setup-templates ()
  "Load VibeLang templates for the current buffer."
  (cond
   ((require 'tempel nil t)
    (add-to-list 'tempel-template-sources 'vibelang-tempel-templates))
   ((require 'yasnippet nil t)
    (let ((snip-dir (expand-file-name "vibelang-snippets"
                                      (file-name-directory (or load-file-name buffer-file-name)))))
      (when (file-directory-p snip-dir)
        (add-to-list 'yas-snippet-dirs snip-dir)
        (yas-load-directory snip-dir))))))

(provide 'vibelang-templates)
;;; vibelang-templates.el ends here
