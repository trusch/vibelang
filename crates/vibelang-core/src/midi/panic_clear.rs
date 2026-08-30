const MIDI_CHANNEL_COUNT: u8 = 16;
const PANIC_CLEAR_CONTROLLERS: [u8; 3] = [64, 123, 120];

pub(crate) fn send_panic_clear(mut send: impl FnMut(&[u8])) {
    for channel in 0..MIDI_CHANNEL_COUNT {
        let status = 0xB0 | channel;
        for controller in PANIC_CLEAR_CONTROLLERS {
            send(&[status, controller, 0]);
        }
    }

    tracing::info!("panic-clear: cheap layer 144 B sync");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cheap_panic_clear_has_expected_order_and_size() {
        let mut sent = Vec::new();
        send_panic_clear(|message| sent.push(<[u8; 3]>::try_from(message).unwrap()));

        let expected: Vec<[u8; 3]> = (0..16u8)
            .flat_map(|channel| {
                let status = 0xB0 | channel;
                [[status, 64, 0], [status, 123, 0], [status, 120, 0]]
            })
            .collect();

        assert_eq!(sent, expected);
        assert_eq!(sent.len(), 16 * 3);
        assert_eq!(sent.len() * 3, 144);
    }
}
