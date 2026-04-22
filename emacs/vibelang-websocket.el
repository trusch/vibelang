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
(require 'subr-x)
(require 'url)
(require 'url-http)

(defconst vibelang-ws-protocol-version 1
  "WebSocket protocol version. Must match WS_PROTOCOL_VERSION in vibelang-http.")

(defconst vibelang--ws-subscription-events
  '("playback.tick" "playback.bar" "transport.*")
  "Playback and transport events the Emacs client subscribes to.")

(defcustom vibelang-ws-stale-threshold 2.5
  "Seconds without playback updates before the connection is marked stale."
  :type 'number
  :group 'vibelang)

(defcustom vibelang-ws-monitor-interval 0.5
  "Seconds between connection health checks."
  :type 'number
  :group 'vibelang)

(defcustom vibelang-ws-reconnect-delay 1.0
  "Seconds to wait before attempting an automatic reconnect."
  :type 'number
  :group 'vibelang)

(defcustom vibelang-api-host nil
  "HTTP API host for VibeLang commands.
When nil, reuse `vibelang-ws-host'."
  :type '(choice (const :tag "Use WebSocket host" nil)
                 string)
  :group 'vibelang)

(defcustom vibelang-api-port nil
  "HTTP API port for VibeLang commands.
When nil, reuse `vibelang-ws-port'."
  :type '(choice (const :tag "Use WebSocket port" nil)
                 integer)
  :group 'vibelang)

(defcustom vibelang-http-timeout 2.0
  "Timeout in seconds for VibeLang HTTP API requests."
  :type 'number
  :group 'vibelang)

(defcustom vibelang-eval-save-before-run t
  "When non-nil, save the current VibeLang buffer before reload/eval commands."
  :type 'boolean
  :group 'vibelang)

(defcustom vibelang-cockpit-log-limit 20
  "Maximum number of recent cockpit command events to retain."
  :type 'integer
  :group 'vibelang)

(defcustom vibelang-command-preview-limit 240
  "Maximum number of preview characters stored for eval commands."
  :type 'integer
  :group 'vibelang)

(defcustom vibelang-ws-max-reconnect-attempts 10
  "Maximum automatic reconnect attempts before giving up.
Nil means keep trying forever."
  :type '(choice (const :tag "Forever" nil)
                 integer)
  :group 'vibelang)

(defcustom vibelang-ws-resync-throttle 1.5
  "Minimum number of seconds between client-side resync requests."
  :type 'number
  :group 'vibelang)

(defvar vibelang--ws-connection nil
  "Active WebSocket connection to VibeLang.")

(defvar vibelang--playback-state nil
  "Current playback state from server.")

(defvar vibelang--last-update-time nil
  "Timestamp of last valid playback state update from the server.")

(defvar vibelang--connected nil
  "Whether the WebSocket transport is currently open.")

(defvar vibelang--ws-host nil
  "Last WebSocket host used by the Emacs client.")

(defvar vibelang--ws-port nil
  "Last WebSocket port used by the Emacs client.")

(defvar vibelang--ws-state 'disconnected
  "Current high-level connection state.
Known values are `disconnected', `connected', `synced', `stale',
`reconnecting', and `protocol-mismatch'.")

(defvar vibelang--ws-state-detail nil
  "Optional human-readable detail for `vibelang--ws-state'.")

(defvar vibelang--ws-state-since nil
  "Timestamp when `vibelang--ws-state' last changed.")

(defvar vibelang--ws-last-message-time nil
  "Timestamp of the last WebSocket message, regardless of payload type.")

(defvar vibelang--ws-last-error nil
  "Last transport or protocol error message.")

(defvar vibelang--ws-reconnect-attempt 0
  "Number of automatic reconnect attempts made for the current session.")

(defvar vibelang--ws-reconnect-timer nil
  "Timer used for delayed WebSocket reconnects.")

(defvar vibelang--ws-monitor-timer nil
  "Timer used to monitor stale playback state.")

(defvar vibelang--ws-last-resync-request-time nil
  "Timestamp of the most recent explicit resync request.")

(defvar vibelang--ws-manual-disconnect nil
  "Non-nil when the current disconnect was initiated locally.")

(defvar vibelang--last-command-result nil
  "Most recent VibeLang command result plist.")

(defvar vibelang--command-log nil
  "Recent VibeLang command/result plists, newest first.")

(defvar vibelang--connection-state-hook nil
  "Hook run when the WebSocket connection state changes.
Called with two arguments: the new state symbol and a detail string or nil.")

(defvar vibelang--playback-state-hook nil
  "Hook called when playback state is updated.
Called with one argument: the playback state alist.")

(defvar vibelang--transport-event-hook nil
  "Hook called on transport events (start/stop/beat).
Called with two arguments: event-type (string) and data (alist).")

(defvar vibelang--command-result-hook nil
  "Hook run when a VibeLang command finishes.
Called with one argument: a result plist.")

(defun vibelang--set-connection-state (state &optional detail)
  "Record WebSocket STATE with optional DETAIL and notify listeners."
  (let ((changed (or (not (eq vibelang--ws-state state))
                     (not (equal vibelang--ws-state-detail detail)))))
    (setq vibelang--ws-state state
          vibelang--ws-state-detail detail)
    (when changed
      (setq vibelang--ws-state-since (float-time))
      (run-hook-with-args 'vibelang--connection-state-hook state detail)
      (force-mode-line-update t))))

(defun vibelang--cancel-reconnect-timer ()
  "Cancel any pending reconnect timer."
  (when (timerp vibelang--ws-reconnect-timer)
    (cancel-timer vibelang--ws-reconnect-timer))
  (setq vibelang--ws-reconnect-timer nil))

(defun vibelang-ws-api-host ()
  "Return the HTTP API host for VibeLang commands."
  (or vibelang-api-host
      (and (boundp 'vibelang-ws-host) vibelang-ws-host)
      vibelang--ws-host
      "127.0.0.1"))

(defun vibelang-ws-api-port ()
  "Return the HTTP API port for VibeLang commands."
  (or vibelang-api-port
      (and (boundp 'vibelang-ws-port) vibelang-ws-port)
      vibelang--ws-port
      1606))

(defun vibelang-ws-api-base-url ()
  "Return the base URL for the VibeLang HTTP API."
  (format "http://%s:%d" (vibelang-ws-api-host) (vibelang-ws-api-port)))

(defun vibelang--command-preview (text)
  "Return a compact preview string for TEXT."
  (let* ((clean (replace-regexp-in-string "[ \t\n\r]+" " " (or text "")))
         (limit (max 32 vibelang-command-preview-limit)))
    (if (> (length clean) limit)
        (concat (substring clean 0 limit) "…")
      clean)))

(defun vibelang--record-command-result (command success message &optional detail payload preview)
  "Record a command result for COMMAND.
SUCCESS indicates whether it succeeded. MESSAGE is short human text.
DETAIL, PAYLOAD, and PREVIEW provide richer cockpit context."
  (let ((result (list :command command
                      :success success
                      :message message
                      :detail detail
                      :payload payload
                      :preview preview
                      :time (current-time))))
    (setq vibelang--last-command-result result
          vibelang--command-log (cons result vibelang--command-log))
    (when (> (length vibelang--command-log) vibelang-cockpit-log-limit)
      (setcdr (nthcdr (1- vibelang-cockpit-log-limit) vibelang--command-log) nil))
    (run-hook-with-args 'vibelang--command-result-hook result)
    result))

(defun vibelang-ws-last-command-result ()
  "Return the most recent recorded VibeLang command result." 
  vibelang--last-command-result)

(defun vibelang-ws-command-log ()
  "Return recent VibeLang command results, newest first."
  vibelang--command-log)

(defun vibelang-ws-host ()
  "Return the current or last-used WebSocket host."
  vibelang--ws-host)

(defun vibelang-ws-port ()
  "Return the current or last-used WebSocket port."
  vibelang--ws-port)

(defun vibelang-ws-state-since ()
  "Return timestamp when the current WebSocket state was entered."
  vibelang--ws-state-since)

(defun vibelang-ws-last-message-time ()
  "Return timestamp of the last received WebSocket message."
  vibelang--ws-last-message-time)

(defun vibelang-ws-last-playback-update-time ()
  "Return timestamp of the last valid playback snapshot update."
  vibelang--last-update-time)

(defun vibelang-ws-last-error ()
  "Return the most recent transport/protocol error string, if any."
  vibelang--ws-last-error)

(defun vibelang--cancel-monitor-timer ()
  "Cancel the connection monitor timer."
  (when (timerp vibelang--ws-monitor-timer)
    (cancel-timer vibelang--ws-monitor-timer))
  (setq vibelang--ws-monitor-timer nil))

(defun vibelang--ensure-monitor-timer ()
  "Start the connection monitor timer if needed."
  (unless (timerp vibelang--ws-monitor-timer)
    (setq vibelang--ws-monitor-timer
          (run-at-time vibelang-ws-monitor-interval
                       vibelang-ws-monitor-interval
                       #'vibelang--ws-monitor-connection))))

(defun vibelang--playback-state-valid-p (data)
  "Return non-nil when DATA looks like a VibeLang playback snapshot."
  (and (listp data)
       (let ((transport (alist-get 'transport data)))
         (and (listp transport)
              (numberp (alist-get 'beat transport))
              (numberp (alist-get 'bar transport))
              (numberp (alist-get 'beat_in_bar transport))
              (numberp (alist-get 'bpm transport))
              (memq (alist-get 'playing transport) '(t nil))))))

(defun vibelang--apply-playback-state (data source)
  "Accept playback DATA from SOURCE and update client state.
If DATA does not match the expected snapshot shape, enter
`protocol-mismatch' state instead of pretending sync is healthy."
  (if (not (vibelang--playback-state-valid-p data))
      (let ((detail (format "Unexpected %s payload shape" source)))
        (setq vibelang--ws-last-error detail)
        (vibelang--set-connection-state 'protocol-mismatch detail)
        (message "VibeLang WebSocket protocol mismatch: %s" detail))
    (setq vibelang--playback-state data
          vibelang--last-update-time (float-time))
    (vibelang--set-connection-state 'synced)
    ;; Run hooks for sidebar and other listeners
    (run-hook-with-args 'vibelang--playback-state-hook data)
    ;; Update visualization directly - ticks are frequent enough
    (vibelang--update-all-visualizations data)
    ;; Update mode line
    (when (fboundp 'vibelang--update-mode-line)
      (vibelang--update-mode-line data))))

(defun vibelang--ws-monitor-connection ()
  "Monitor the current WebSocket session for stale state."
  (cond
   ((not vibelang--connected)
    (vibelang--cancel-monitor-timer))
   ((and vibelang--last-update-time
         (> (- (float-time) vibelang--last-update-time)
            vibelang-ws-stale-threshold))
    (unless (eq vibelang--ws-state 'stale)
      (vibelang--set-connection-state
       'stale
       (format "No playback update for %.1fs"
               (- (float-time) vibelang--last-update-time))))
    (vibelang-ws-request-resync "playback stream went stale"))
   ((and (eq vibelang--ws-state 'stale) vibelang--last-update-time)
    (vibelang--set-connection-state 'synced))))

(defun vibelang--schedule-reconnect (&optional reason)
  "Schedule an automatic reconnect, optionally annotating with REASON."
  (when (and (not vibelang--ws-manual-disconnect)
             vibelang--ws-host
             vibelang--ws-port
             (or (null vibelang-ws-max-reconnect-attempts)
                 (< vibelang--ws-reconnect-attempt vibelang-ws-max-reconnect-attempts)))
    (setq vibelang--ws-reconnect-attempt (1+ vibelang--ws-reconnect-attempt))
    (vibelang--cancel-reconnect-timer)
    (vibelang--set-connection-state
     'reconnecting
     (format "Attempt %d in %.1fs%s"
             vibelang--ws-reconnect-attempt
             vibelang-ws-reconnect-delay
             (if (and reason (not (string-empty-p reason)))
                 (format " (%s)" reason)
               "")))
    (setq vibelang--ws-reconnect-timer
          (run-at-time vibelang-ws-reconnect-delay nil
                       (lambda ()
                         (setq vibelang--ws-reconnect-timer nil)
                         (when (and (not vibelang--connected)
                                    (not vibelang--ws-manual-disconnect))
                           (vibelang-ws-connect vibelang--ws-host vibelang--ws-port)))))))

(defun vibelang--ws-send-subscription ()
  "Send the standard playback subscription request."
  (vibelang--ws-send-json
   `((action . "subscribe")
     (events . ,(vconcat vibelang--ws-subscription-events)))))

(defun vibelang-ws-connect (host port)
  "Connect to VibeLang WebSocket server at HOST:PORT."
  (setq vibelang--ws-host host
        vibelang--ws-port port
        vibelang--ws-manual-disconnect nil)
  (vibelang--cancel-reconnect-timer)
  (when vibelang--ws-connection
    (let ((vibelang--ws-manual-disconnect t))
      (vibelang-ws-disconnect)))
  (let ((url (format "ws://%s:%d/ws" host port)))
    (condition-case err
        (progn
          (setq vibelang--ws-last-error nil)
          (vibelang--set-connection-state 'connected "Opening WebSocket")
          (message "Opening WebSocket to %s..." url)
          (setq vibelang--ws-connection
                (websocket-open url
                  :on-message #'vibelang--ws-on-message
                  :on-close #'vibelang--ws-on-close
                  :on-error #'vibelang--ws-on-error
                  :on-open #'vibelang--ws-on-open))
          (message "WebSocket connection initiated to %s" url))
      (error
       (setq vibelang--ws-connection nil
             vibelang--connected nil
             vibelang--ws-last-error (error-message-string err))
       (vibelang--set-connection-state 'disconnected vibelang--ws-last-error)
       (message "Failed to connect to VibeLang at %s: %s" url (error-message-string err))))))

(defun vibelang-ws-disconnect ()
  "Disconnect from VibeLang WebSocket server."
  (interactive)
  (setq vibelang--ws-manual-disconnect t)
  (vibelang--cancel-reconnect-timer)
  (vibelang--cancel-monitor-timer)
  (when vibelang--ws-connection
    (websocket-close vibelang--ws-connection)
    (setq vibelang--ws-connection nil))
  (setq vibelang--connected nil
        vibelang--playback-state nil)
  (vibelang--set-connection-state 'disconnected "Disconnected")
  (vibelang--clear-visualization)
  (when (fboundp 'vibelang--update-mode-line)
    (vibelang--update-mode-line nil))
  (message "Disconnected from VibeLang"))

(defun vibelang--ws-on-open (_ws)
  "Handle WebSocket connection opened."
  (setq vibelang--connected t
        vibelang--ws-last-error nil
        vibelang--ws-reconnect-attempt 0)
  (vibelang--set-connection-state 'connected "Waiting for initial snapshot")
  (vibelang--ensure-monitor-timer)
  (vibelang--ws-send-subscription)
  (message "Connected to VibeLang"))

(defun vibelang--handle-hello (data)
  "Handle hello handshake event, checking protocol version."
  (let ((server-version (alist-get 'protocol_version data))
        (client-version vibelang-ws-protocol-version))
    (if (and server-version (= server-version client-version))
        (message "VibeLang: connected (protocol v%d)" server-version)
      (let ((detail (format "server protocol v%s, client expects v%d"
                            (or server-version "unknown") client-version)))
        (vibelang--set-connection-state 'protocol-mismatch detail)
        (message "VibeLang: protocol mismatch — %s" detail)))))

(defun vibelang--ws-on-message (_ws frame)
  "Handle incoming WebSocket FRAME."
  (let* ((payload (websocket-frame-text frame))
         (data (condition-case nil
                   (let ((json-object-type 'alist)
                         (json-array-type 'list)
                         (json-key-type 'symbol)
                         (json-false nil)
                         (json-null nil))
                     (json-read-from-string payload))
                 (error nil))))
    (setq vibelang--ws-last-message-time (float-time))
    (if (not data)
        (let ((detail "Received non-JSON WebSocket payload"))
          (setq vibelang--ws-last-error detail)
          (vibelang--set-connection-state 'protocol-mismatch detail))
      (let ((event-type (alist-get 'type data))
            (event-data (alist-get 'data data)))
        (cond
         ((string= event-type "hello")
          (vibelang--handle-hello event-data))
         ;; Server polls at 20 Hz and emits playback.tick when each 16th-note
         ;; boundary is crossed. At 120 BPM this yields ~8 ticks/beat; at very
         ;; high tempos (>240 BPM) ticks may occasionally merge.
         ((string= event-type "playback.tick")
          (vibelang--handle-playback-tick event-data))
         ;; playback.bar fires on bar changes and is also used as the initial snapshot
         ((string= event-type "playback.bar")
          (vibelang--handle-playback-bar event-data))
         ((string-prefix-p "transport." event-type)
          (vibelang--handle-transport-event event-type event-data)))))))

(defun vibelang--ws-on-close (_ws)
  "Handle WebSocket connection closed."
  (setq vibelang--connected nil
        vibelang--ws-connection nil)
  (vibelang--cancel-monitor-timer)
  (vibelang--clear-visualization)
  (if (eq vibelang--ws-state 'protocol-mismatch)
      (message "VibeLang: runtime version mismatch — restart runtime to reconnect")
    (unless vibelang--ws-manual-disconnect
      (vibelang--schedule-reconnect "socket closed"))
    (unless (eq vibelang--ws-state 'reconnecting)
      (vibelang--set-connection-state 'disconnected "Socket closed"))
    (message "VibeLang WebSocket connection closed")))

(defun vibelang--ws-on-error (_ws type err)
  "Handle WebSocket error ERR of TYPE."
  (setq vibelang--connected nil
        vibelang--ws-last-error (format "%s" err))
  (vibelang--cancel-monitor-timer)
  (unless vibelang--ws-manual-disconnect
    (vibelang--schedule-reconnect (format "%s" type)))
  (unless (eq vibelang--ws-state 'reconnecting)
    (vibelang--set-connection-state 'disconnected (format "%s" err)))
  (message "VibeLang WebSocket error (%s): %s" type err))

(defun vibelang--ws-send-json (data)
  "Send DATA as JSON through WebSocket."
  (when (and vibelang--ws-connection vibelang--connected)
    (websocket-send-text vibelang--ws-connection
                         (json-encode data))))

(defun vibelang--http-json-read (string)
  "Parse STRING as JSON and return an alist or scalar.
Return nil when parsing fails or STRING is blank."
  (when (and string (not (string-empty-p string)))
    (condition-case nil
        (let ((json-object-type 'alist)
              (json-array-type 'list)
              (json-key-type 'symbol)
              (json-false nil)
              (json-null nil))
          (json-read-from-string string))
      (error nil))))

(defun vibelang--http-request (method path &optional payload)
  "Send an HTTP request with METHOD to PATH and return a result plist.
PAYLOAD, when non-nil, is encoded as JSON."
  (let* ((url-request-method method)
         (url-request-extra-headers
          `(("Accept" . "application/json")
            ,@(when payload '(("Content-Type" . "application/json")))))
         (url-request-data (when payload (json-encode payload)))
         (url (concat (vibelang-ws-api-base-url) path))
         (buffer (url-retrieve-synchronously url t t vibelang-http-timeout)))
    (if (not buffer)
        (list :ok nil :status nil :body nil :json nil
              :error (format "No response from %s" url))
      (unwind-protect
          (with-current-buffer buffer
            (let* ((status (or (and (boundp 'url-http-response-status) url-http-response-status) 0))
                   (body (progn
                           (goto-char (or (and (boundp 'url-http-end-of-headers)
                                               url-http-end-of-headers)
                                          (point-min)))
                           (buffer-substring-no-properties (point) (point-max))))
                   (json (vibelang--http-json-read body))
                   (ok (and (>= status 200) (< status 300)))
                   (error-msg (or (alist-get 'error json)
                                  (unless ok (format "HTTP %s" status)))))
              (list :ok ok :status status :body body :json json :error error-msg)))
        (kill-buffer buffer)))))

(defun vibelang-ws-request-resync (&optional reason)
  "Request a fresh playback snapshot from the server.
Optional REASON is only used for messages and diagnostics."
  (interactive)
  (when (and vibelang--connected
             (or (not vibelang--ws-last-resync-request-time)
                 (> (- (float-time) vibelang--ws-last-resync-request-time)
                    vibelang-ws-resync-throttle)))
    (setq vibelang--ws-last-resync-request-time (float-time))
    (vibelang--ws-send-subscription)
    (message "VibeLang resync requested%s"
             (if (and reason (not (string-empty-p reason)))
                 (format ": %s" reason)
               ""))))

(defun vibelang-http-request (method path &optional payload)
  "Send HTTP METHOD to PATH with optional JSON PAYLOAD.
This is the public wrapper used by other Emacs UI modules."
  (let* ((result (vibelang--http-request method path payload))
         (json (plist-get result :json)))
    (list :ok (plist-get result :ok)
          :status (plist-get result :status)
          :data json
          :body (plist-get result :body)
          :error (plist-get result :error))))

(defun vibelang-ws-api-ready-p ()
  "Return non-nil when the VibeLang HTTP API responds."
  (plist-get (vibelang--http-request "GET" "/transport") :ok))

(defun vibelang-ws-eval-string (code &optional label)
  "Evaluate CODE through the VibeLang HTTP API.
LABEL is used for cockpit/status messaging. Returns a result plist."
  (let* ((preview (vibelang--command-preview code))
         (response (vibelang--http-request "POST" "/eval" `((code . ,code))))
         (json (plist-get response :json))
         (ok (and (plist-get response :ok) (alist-get 'success json)))
         (message-text (if ok
                           (or (alist-get 'result json) "Code executed successfully")
                         (or (alist-get 'error json)
                             (plist-get response :error)
                             "Evaluation failed")))
         (result (vibelang--record-command-result
                  (or label "eval") ok message-text nil json preview)))
    (message "VibeLang %s: %s"
             (if ok "eval ok" "eval failed")
             message-text)
    result))

(defun vibelang-ws-send-command (command &optional data)
  "Send COMMAND with optional DATA to VibeLang via the HTTP API.
Returns a command-result plist for cockpit consumers."
  (let* ((result
          (pcase command
            ("transport.start"
             (let* ((response (vibelang--http-request "POST" "/transport/start"))
                    (ok (plist-get response :ok))
                    (msg (if ok
                             "Transport started"
                           (or (plist-get response :error) "Failed to start transport"))))
               (vibelang--record-command-result command ok msg nil
                                                (plist-get response :json))))
            ("transport.stop"
             (let* ((response (vibelang--http-request "POST" "/transport/stop"))
                    (ok (plist-get response :ok))
                    (msg (if ok
                             "Transport stopped"
                           (or (plist-get response :error) "Failed to stop transport"))))
               (vibelang--record-command-result command ok msg nil
                                                (plist-get response :json))))
            ("eval.reload"
             (let* ((file (alist-get 'file data))
                    (code (or (alist-get 'code data)
                              (and file
                                   (with-temp-buffer
                                     (insert-file-contents file)
                                     (buffer-string))))))
               (if (not code)
                   (vibelang--record-command-result command nil
                                                   "No code or file provided for reload")
                 (vibelang-ws-eval-string code (format "%s (%s)" command
                                                       (or (and file (file-name-nondirectory file))
                                                           "buffer"))))))
            (_
             (vibelang--record-command-result command nil
                                             (format "Unsupported command: %s" command)))))
         (ok (plist-get result :success))
         (msg (plist-get result :message)))
    (unless (string= command "eval.reload")
      (message "VibeLang %s: %s"
               (if ok "ok" "error")
               msg))
    result))

;;; Event handlers

(defun vibelang--handle-playback-tick (data)
  "Handle playback.tick event with DATA.
This fires every 16th note for smooth visualization updates."
  (vibelang--apply-playback-state data "playback.tick"))

(defun vibelang--handle-playback-bar (data)
  "Handle playback.bar event with DATA.
This fires on bar changes and is also used as the initial snapshot."
  (vibelang--apply-playback-state data "playback.bar"))

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
  "Return non-nil if the WebSocket transport is currently open."
  vibelang--connected)

(defun vibelang-ws-connection-state ()
  "Return the current high-level WebSocket connection state symbol."
  vibelang--ws-state)

(defun vibelang-ws-state-label (&optional state)
  "Return a user-facing label for WebSocket STATE.
If STATE is nil, use the current connection state."
  (pcase (or state vibelang--ws-state)
    ('connected "CONNECTED")
    ('synced "SYNCED")
    ('stale "STALE")
    ('reconnecting "RECONNECTING")
    ('protocol-mismatch "PROTOCOL MISMATCH")
    (_ "DISCONNECTED")))

(defun vibelang-ws-status-string ()
  "Return a concise, human-readable WebSocket status string."
  (let ((label (vibelang-ws-state-label)))
    (if (and vibelang--ws-state-detail
             (not (string-empty-p vibelang--ws-state-detail)))
        (format "%s — %s" label vibelang--ws-state-detail)
      label)))

(defun vibelang-ws-playback-state ()
  "Return current playback state or nil."
  vibelang--playback-state)

(defun vibelang-ws-time-signature-components (time-sig)
  "Return (NUM DENOM) extracted from TIME-SIG.
TIME-SIG may be a list, vector, or nil. Falls back to 4/4."
  (cond
   ((vectorp time-sig)
    (list (or (and (> (length time-sig) 0) (aref time-sig 0)) 4)
          (or (and (> (length time-sig) 1) (aref time-sig 1)) 4)))
   ((listp time-sig)
    (list (or (car time-sig) 4)
          (or (cadr time-sig) 4)))
   (t
    '(4 4))))

(defun vibelang--seconds-ago-string (timestamp)
  "Return a human-readable age string for TIMESTAMP."
  (if (not timestamp)
      "never"
    (format "%.1fs ago" (- (float-time) timestamp))))

(defun vibelang-ws-diagnose ()
  "Diagnose WebSocket connection status and configuration."
  (interactive)
  (with-current-buffer (get-buffer-create "*VibeLang Diagnostics*")
    (let ((host (or vibelang--ws-host
                    (and (boundp 'vibelang-ws-host) vibelang-ws-host)
                    "localhost"))
          (port (or vibelang--ws-port
                    (and (boundp 'vibelang-ws-port) vibelang-ws-port)
                    1606)))
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
      (insert (format "   Host: %s\n" host))
      (insert (format "   Port: %s\n" port))
      (insert (format "   URL: ws://%s:%d/ws\n" host port))
      (insert (format "   Subscriptions: %s\n"
                      (string-join vibelang--ws-subscription-events ", ")))
      (insert (format "   Stale threshold: %.1fs\n" vibelang-ws-stale-threshold))
      (insert (format "   Reconnect delay: %.1fs\n" vibelang-ws-reconnect-delay))
      (insert (format "   Max reconnect attempts: %s\n"
                      (if vibelang-ws-max-reconnect-attempts
                          (number-to-string vibelang-ws-max-reconnect-attempts)
                        "forever")))

      ;; Connection status
      (insert "\n3. Connection Status:\n")
      (insert (format "   Transport open: %s\n" (if vibelang--connected "YES" "NO")))
      (insert (format "   High-level state: %s\n" (vibelang-ws-status-string)))
      (insert (format "   State since: %s\n" (vibelang--seconds-ago-string vibelang--ws-state-since)))
      (insert (format "   Last message: %s\n" (vibelang--seconds-ago-string vibelang--ws-last-message-time)))
      (insert (format "   Last playback update: %s\n" (vibelang--seconds-ago-string vibelang--last-update-time)))
      (insert (format "   Last resync request: %s\n" (vibelang--seconds-ago-string vibelang--ws-last-resync-request-time)))
      (insert (format "   Manual disconnect: %s\n" (if vibelang--ws-manual-disconnect "YES" "NO")))
      (insert (format "   Reconnect attempts: %d\n" vibelang--ws-reconnect-attempt))
      (insert (format "   Reconnect timer active: %s\n" (if (timerp vibelang--ws-reconnect-timer) "YES" "NO")))
      (insert (format "   Monitor timer active: %s\n" (if (timerp vibelang--ws-monitor-timer) "YES" "NO")))
      (insert (format "   Connection object: %s\n"
                      (if vibelang--ws-connection "exists" "nil")))
      (when vibelang--ws-connection
        (insert (format "   Ready state: %s\n"
                        (condition-case nil
                            (websocket-ready-state vibelang--ws-connection)
                          (error "unknown")))))
      (when vibelang--ws-last-error
        (insert (format "   Last error: %s\n" vibelang--ws-last-error)))

      ;; Playback state
      (insert "\n4. Playback State:\n")
      (if vibelang--playback-state
          (let* ((transport (alist-get 'transport vibelang--playback-state))
                 (playing (alist-get 'playing transport))
                 (bar (alist-get 'bar transport))
                 (beat-in-bar (alist-get 'beat_in_bar transport))
                 (bpm (alist-get 'bpm transport)))
            (insert (format "   Playing: %s\n" (if playing "YES" "NO")))
            (insert (format "   Position: %s\n"
                            (if (and bar beat-in-bar)
                                (format "%d.%d" (1+ bar) (1+ beat-in-bar))
                              "unknown")))
            (insert (format "   BPM: %s\n" (or bpm "unknown")))
            (insert (format "   Snapshot keys: %s\n"
                            (mapconcat (lambda (cell) (symbol-name (car cell)))
                                       vibelang--playback-state ", "))))
        (insert "   No playback state received yet\n"))

      ;; Network check
      (insert "\n5. Network Check:\n")
      (condition-case err
          (let ((proc (make-network-process
                       :name "vibelang-test"
                       :host host
                       :service port
                       :nowait nil
                       :coding 'binary)))
            (delete-process proc)
            (insert (format "   [OK] Port %d is accepting TCP connections\n" port)))
        (error
         (insert (format "   [ERROR] Cannot connect to %s:%d - %s\n"
                         host port (error-message-string err)))))

      ;; Terminal info
      (insert "\n6. Terminal Info:\n")
      (insert (format "   TERM: %s\n" (getenv "TERM")))
      (insert (format "   Display colors: %s\n" (display-color-cells)))
      (insert (format "   Truecolor: %s\n"
                      (if (>= (display-color-cells) 16777216) "YES" "NO (colors may look different)")))

      ;; Suggestions
      (insert "\n7. Suggestions:\n")
      (cond
       ((not (featurep 'websocket))
        (insert "   - Install websocket package: M-x package-install RET websocket RET\n"))
       ((eq vibelang--ws-state 'protocol-mismatch)
        (insert "   - The server is speaking an unexpected payload shape; update the Emacs client or runtime so they match\n")
        (insert "   - Inspect the raw runtime/WebSocket changes around playback snapshots\n"))
       ((eq vibelang--ws-state 'stale)
        (insert "   - The socket is open but playback updates stopped arriving\n")
        (insert "   - Try M-x vibelang-ws-request-resync or reconnect if the runtime recently reloaded\n"))
       ((eq vibelang--ws-state 'reconnecting)
        (insert "   - The client is already trying to reconnect; wait a moment or check whether the runtime restarted\n"))
       ((not vibelang--connected)
        (insert "   - Make sure VibeLang runtime is running: vibe run your-script.vibe\n")
        (insert "   - API is enabled by default (use --no-api to disable)\n")
        (insert "   - Check if port 1606 is not blocked by firewall\n")
        (insert "   - For SSH: ensure TERM supports colors (try: export TERM=xterm-256color)\n"))
       ((eq vibelang--ws-state 'connected)
        (insert "   - Transport is up but the client is still waiting for a playback snapshot\n")
        (insert "   - If this persists, request a resync or verify the runtime's WebSocket payloads\n"))
       (t
        (insert "   Connection looks healthy.\n")))

      (goto-char (point-min))
      (display-buffer (current-buffer)))))

(provide 'vibelang-websocket)
;;; vibelang-websocket.el ends here
