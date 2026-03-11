;;; vibelang-websocket.el --- WebSocket client for VibeLang -*- lexical-binding: t -*-

;; Copyright (C) 2024 VibeLang Authors
;; Author: VibeLang Team
;; Keywords: comm, music

;;; Commentary:
;; WebSocket client for connecting to VibeLang runtime and receiving
;; real-time playback state updates for visualization.

;;; Code:

(require 'websocket)
(require 'json)

(defvar vibelang--ws-connection nil
  "Active WebSocket connection to VibeLang.")

(defvar vibelang--playback-state nil
  "Current playback state from server.")

(defvar vibelang--last-update-time nil
  "Timestamp of last playback state update.")

(defvar vibelang--connected nil
  "Whether we're currently connected to VibeLang.")

(defvar vibelang--playback-state-hook nil
  "Hook called when playback state is updated.
Called with one argument: the playback state alist.")

(defvar vibelang--transport-event-hook nil
  "Hook called on transport events (start/stop/beat).
Called with two arguments: event-type (string) and data (alist).")

(defun vibelang-ws-connect (host port)
  "Connect to VibeLang WebSocket server at HOST:PORT."
  (when vibelang--ws-connection
    (vibelang-ws-disconnect))
  (let ((url (format "ws://%s:%d/ws" host port)))
    (condition-case err
        (progn
          (message "Opening WebSocket to %s..." url)
          (setq vibelang--ws-connection
                (websocket-open url
                  :on-message #'vibelang--ws-on-message
                  :on-close #'vibelang--ws-on-close
                  :on-error #'vibelang--ws-on-error
                  :on-open #'vibelang--ws-on-open))
          (message "WebSocket connection initiated to %s" url))
      (error
       (setq vibelang--ws-connection nil)
       (setq vibelang--connected nil)
       (message "Failed to connect to VibeLang at %s: %s" url (error-message-string err))))))

(defun vibelang-ws-disconnect ()
  "Disconnect from VibeLang WebSocket server."
  (when vibelang--ws-connection
    (websocket-close vibelang--ws-connection)
    (setq vibelang--ws-connection nil))
  (setq vibelang--connected nil)
  (setq vibelang--playback-state nil)
  (vibelang--clear-visualization)
  (message "Disconnected from VibeLang"))

(defun vibelang--ws-on-open (_ws)
  "Handle WebSocket connection opened."
  (setq vibelang--connected t)
  ;; Subscribe to playback events
  ;; playback.tick fires on every 16th note (~8/beat) - sufficient for smooth viz
  ;; playback.bar fires on bar changes (less frequent, for backwards compat)
  (vibelang--ws-send-json
   `((action . "subscribe")
     (events . ("playback.tick" "playback.bar" "transport.*"))))
  (message "Connected to VibeLang"))

(defun vibelang--ws-on-message (_ws frame)
  "Handle incoming WebSocket FRAME."
  (let* ((payload (websocket-frame-text frame))
         (data (condition-case nil
                   (json-read-from-string payload)
                 (error nil))))
    (when data
      (let ((event-type (alist-get 'type data))
            (event-data (alist-get 'data data)))
        (cond
         ;; playback.tick fires every 16th note - use for smooth visualization
         ((string= event-type "playback.tick")
          (vibelang--handle-playback-tick event-data))
         ;; playback.bar fires on bar changes - fallback for older servers
         ((string= event-type "playback.bar")
          (vibelang--handle-playback-bar event-data))
         ((string-prefix-p "transport." event-type)
          (vibelang--handle-transport-event event-type event-data)))))))

(defun vibelang--ws-on-close (_ws)
  "Handle WebSocket connection closed."
  (setq vibelang--connected nil)
  (setq vibelang--ws-connection nil)
  (vibelang--clear-visualization)
  (message "VibeLang WebSocket connection closed"))

(defun vibelang--ws-on-error (_ws type err)
  "Handle WebSocket error ERR of TYPE."
  (setq vibelang--connected nil)
  (message "VibeLang WebSocket error (%s): %s" type err))

(defun vibelang--ws-send-json (data)
  "Send DATA as JSON through WebSocket."
  (when (and vibelang--ws-connection vibelang--connected)
    (websocket-send-text vibelang--ws-connection
                         (json-encode data))))

(defun vibelang-ws-send-command (command &optional data)
  "Send COMMAND with optional DATA to VibeLang.
Note: Commands require HTTP API, this is a placeholder for future
bidirectional WebSocket command support."
  (message "Command '%s' requires HTTP API (not yet implemented)" command))

;;; Event handlers

(defun vibelang--handle-playback-tick (data)
  "Handle playback.tick event with DATA.
This fires every 16th note for smooth visualization updates."
  (setq vibelang--playback-state data)
  (setq vibelang--last-update-time (float-time))
  ;; Run hooks for sidebar and other listeners
  (run-hook-with-args 'vibelang--playback-state-hook data)
  ;; Update visualization directly - ticks are frequent enough
  (vibelang--update-all-visualizations data)
  ;; Update mode line
  (when (fboundp 'vibelang--update-mode-line)
    (vibelang--update-mode-line data)))

(defun vibelang--handle-playback-bar (data)
  "Handle playback.bar event with DATA.
This fires on bar changes (backwards compat for older servers)."
  (setq vibelang--playback-state data)
  (setq vibelang--last-update-time (float-time))
  ;; Run hooks for sidebar and other listeners
  (run-hook-with-args 'vibelang--playback-state-hook data)
  ;; Update visualization (fallback for servers without tick events)
  (vibelang--update-all-visualizations data)
  ;; Update mode line
  (when (fboundp 'vibelang--update-mode-line)
    (vibelang--update-mode-line data)))

(defun vibelang--handle-transport-event (event-type data)
  "Handle transport EVENT-TYPE with DATA."
  (run-hook-with-args 'vibelang--transport-event-hook event-type data)
  (cond
   ((string= event-type "transport.started")
    (message "VibeLang: Playback started at beat %.1f"
             (alist-get 'beat data)))
   ((string= event-type "transport.stopped")
    (message "VibeLang: Playback stopped")
    (vibelang--clear-visualization)
    (when (fboundp 'vibelang--update-mode-line)
      (vibelang--update-mode-line nil)))))

;;; Visualization updates (delegated to vibelang-visualization.el)

(defun vibelang--update-all-visualizations (data)
  "Update all visualizations based on playback DATA."
  (when (fboundp 'vibelang-viz--update-all)
    (vibelang-viz--update-all data)))

(defun vibelang--clear-visualization ()
  "Clear all visualization overlays."
  (when (fboundp 'vibelang-viz--clear-all)
    (vibelang-viz--clear-all)))

;;; Status functions

(defun vibelang-ws-connected-p ()
  "Return non-nil if connected to VibeLang."
  vibelang--connected)

(defun vibelang-ws-playback-state ()
  "Return current playback state or nil."
  vibelang--playback-state)

(defun vibelang-ws-diagnose ()
  "Diagnose WebSocket connection status and configuration."
  (interactive)
  (with-current-buffer (get-buffer-create "*VibeLang Diagnostics*")
    (erase-buffer)
    (insert "VibeLang WebSocket Diagnostics\n")
    (insert "==============================\n\n")

    ;; Check websocket package
    (insert "1. WebSocket Package:\n")
    (if (featurep 'websocket)
        (insert "   [OK] websocket package is loaded\n")
      (insert "   [ERROR] websocket package not loaded!\n"))

    ;; Configuration
    (insert "\n2. Configuration:\n")
    (insert (format "   Host: %s\n" (if (boundp 'vibelang-ws-host) vibelang-ws-host "NOT SET")))
    (insert (format "   Port: %s\n" (if (boundp 'vibelang-ws-port) vibelang-ws-port "NOT SET")))
    (insert (format "   URL: ws://%s:%d/ws\n"
                    (if (boundp 'vibelang-ws-host) vibelang-ws-host "localhost")
                    (if (boundp 'vibelang-ws-port) vibelang-ws-port 1606)))

    ;; Connection status
    (insert "\n3. Connection Status:\n")
    (insert (format "   Connected: %s\n" (if vibelang--connected "YES" "NO")))
    (insert (format "   Connection object: %s\n"
                    (if vibelang--ws-connection "exists" "nil")))
    (when vibelang--ws-connection
      (insert (format "   Connection state: %s\n"
                      (condition-case nil
                          (websocket-ready-state vibelang--ws-connection)
                        (error "unknown")))))

    ;; Playback state
    (insert "\n4. Playback State:\n")
    (if vibelang--playback-state
        (insert (format "   %S\n" vibelang--playback-state))
      (insert "   No playback state received yet\n"))

    ;; Network check
    (insert "\n5. Network Check:\n")
    (let ((host (if (boundp 'vibelang-ws-host) vibelang-ws-host "localhost"))
          (port (if (boundp 'vibelang-ws-port) vibelang-ws-port 1606)))
      (condition-case err
          (let ((proc (make-network-process
                       :name "vibelang-test"
                       :host host
                       :service port
                       :nowait nil)))
            (delete-process proc)
            (insert (format "   [OK] Port %d is accepting connections\n" port)))
        (error
         (insert (format "   [ERROR] Cannot connect to %s:%d - %s\n"
                         host port (error-message-string err))))))

    ;; Terminal info
    (insert "\n6. Terminal Info:\n")
    (insert (format "   TERM: %s\n" (getenv "TERM")))
    (insert (format "   Display colors: %s\n" (display-color-cells)))
    (insert (format "   Truecolor: %s\n"
                    (if (>= (display-color-cells) 16777216) "YES" "NO (colors may look different)")))

    (insert "\n7. Suggestions:\n")
    (cond
     ((not (featurep 'websocket))
      (insert "   - Install websocket package: M-x package-install RET websocket RET\n"))
     ((not vibelang--connected)
      (insert "   - Make sure VibeLang runtime is running: vibe run your-script.vibe\n")
      (insert "   - API is enabled by default (use --no-api to disable)\n")
      (insert "   - Check if port 1606 is not blocked by firewall\n")
      (insert "   - For SSH: ensure TERM supports colors (try: export TERM=xterm-256color)\n"))
     (t
      (insert "   Connection looks good!\n")))

    (display-buffer (current-buffer))))

(provide 'vibelang-websocket)
;;; vibelang-websocket.el ends here
