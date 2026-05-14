# Verbos West-Coast Manual Sources

Official Verbos Electronics sources for the west-coast rack research wave. PDF
copies were cached locally under `target/tmp/verbos-west-coast-manuals/` for
text extraction; that directory is gitignored and is not part of this manifest.

| Module | Official product page | Official manual / tech-card URL | Notes |
|---|---|---|---|
| Harmonic Oscillator | https://verboselectronics.com/harmonic-oscillator/ | https://verboselectronics.com/wp-content/uploads/2025/10/HarmonicOscillatorTechCard.pdf | Current WordPress media attachment, title `Harmonic+Oscillator+Tech+Card`. |
| Multi-Delay Processor | https://verboselectronics.com/multi-delay-processor/ | https://verboselectronics.com/wp-content/uploads/2025/10/Multi-DelayProcessorTechCard.pdf | Current WordPress media attachment, title `Multi-Delay+Processor+Tech+Card`. The old `www.verboselectronics.com/modules/multi-delay-processor` page now redirects to the rebuilt site. |
| Random Sampling | https://verboselectronics.com/random-sampling/ | https://verboselectronics.com/tech-cards/Random%2BSampling%2BTech%2BCard.pdf | Live official tech-card endpoint. The older Squarespace mirror `https://www.verboselectronics.com/s/Random-Sampling-Tech-Card.pdf` also resolves. |
| Voltage Multistage | https://verboselectronics.com/voltage-multistage/ | https://verboselectronics.com/tech-cards/Voltage%2BMultistage%2BTech%2BCard.pdf | Live official tech-card endpoint. The older Squarespace mirror `https://www.verboselectronics.com/s/Voltage-Multistage-Tech-Card.pdf` also resolves. |
| Touchplate Keyboard | https://verboselectronics.com/touchplate-keyboard/ | https://verboselectronics.com/wp-content/uploads/2025/10/TouchplateKeyboardTechCard.pdf | Current WordPress media attachment, title `Touchplate+Keyboard+Tech+Card`. |
| Bark Filter Processor | https://verboselectronics.com/bark-filter-processor/ | https://verboselectronics.com/wp-content/uploads/2025/10/BarkFilterProcessorTechcard.pdf | Current WordPress media attachment, title `Bark+Filter+Processor+Tech+card`. The older Squarespace mirror `https://www.verboselectronics.com/s/Bark-Filter-Processor-Tech-card.pdf` also resolves. |
| Amp & Tone (Amplitude & Tone Controller 2020) | https://verboselectronics.com/amp-tone-2020-version/ | https://verboselectronics.com/wp-content/uploads/2025/10/AMPTONETechCard.pdf | Current WordPress media attachment, title `AMP+&+TONE+Tech+Card`. This is the current 10HP Amp & Tone version of the Amplitude & Tone Controller role. |

## Source Caveats

- Verbos moved from older `www.verboselectronics.com/modules/...` Squarespace pages to the 2026 WordPress site. Some indexed product URLs still show useful snippets in search caches but redirect to rebuilt pages or `Under Construction` when fetched directly.
- The official Bark Filter Processor tech card documents 12 fixed Bark-scale filters: lowpass at 100 Hz, 10 bandpass filters from 300 Hz through 7.7 kHz, and highpass at 10.5 kHz. If the rack epic keeps the phrase "24-band fixed-Bark-scale filter bank", implementation should clarify whether that means 24 actual bands or the 12 audio bands plus 12 envelope-follower/control lanes.
