;;; vibelang-transient.el --- Transient command menu for VibeLang -*- lexical-binding: t -*-

;; Copyright (C) 2024 VibeLang Authors

;;; Commentary:
;; Unified transient command menu bound to C-c C-. in vibelang-mode.

;;; Code:

(require 'transient)

;; Stubs for functions implemented in parallel agents
(declare-function vibelang-bpm-nudge-up "vibelang-mode")
(declare-function vibelang-bpm-nudge-down "vibelang-mode")
(declare-function vibelang-set-bpm "vibelang-mode")
(declare-function vibelang-describe-entity "vibelang-mode")
(declare-function vibelang-edit-param-at-point "vibelang-mode")
(defvar vibelang-auto-reload-on-save)

(defun vibelang-toggle-auto-reload ()
  "Toggle `vibelang-auto-reload-on-save' and report."
  (interactive)
  (setq vibelang-auto-reload-on-save (not vibelang-auto-reload-on-save))
  (message "Auto-reload on save: %s" (if vibelang-auto-reload-on-save "on" "off")))

(transient-define-prefix vibelang-menu ()
  "VibeLang command menu."
  ["Transport"
   ("p" "Play"         vibelang-transport-start)
   ("x" "Stop"         vibelang-transport-stop)
   ("+" "BPM +"        vibelang-bpm-nudge-up)
   ("-" "BPM -"        vibelang-bpm-nudge-down)
   ("b" "Set BPM"      vibelang-set-bpm)
   ("t" "Tap tempo"    vibelang-tap-tempo)]
  ["Entity"
   ("m" "Mute"         vibelang-mute-at-point)
   ("s" "Solo"         vibelang-solo-at-point)
   ("i" "Inspect"      vibelang-describe-entity)
   ("P" "Edit param"   vibelang-edit-param-at-point)]
  ["Session"
   ("c" "Connect"      vibelang-connect)
   ("d" "Disconnect"   vibelang-disconnect)
   ("r" "Reload"       vibelang-reload-script)
   ("R" "Restart"      vibelang-restart-runtime)
   ("A" "Auto-reload"  vibelang-toggle-auto-reload :transient t)]
  ["Navigate"
   ("/" "Jump to…"     imenu)
   ("B" "Sidebar"      vibelang-sidebar-toggle)
   ("." "Cockpit"      vibelang-cockpit-open)
   ("?" "Diagnose"     vibelang-ws-diagnose)])

(provide 'vibelang-transient)
;;; vibelang-transient.el ends here
