# 4ms Ambient Rack Manual Sources

Research date: 2026-05-14

Scope: official 4ms source pages and downloadable manuals for the modules named in the 4ms ambient rack epic. PDFs were cached locally under `.ewa-artifacts/4ms-ambient/manuals/`, which is ignored by Git in this worktree. Only this source manifest is intended for commit.

## Source Manifest

| Module | Official product/source page | Manual URL | Cached file | Notes |
|---|---|---|---|---|
| Spectral Multiband Resonator (SMR) | https://4mscompany.com/smr.php | https://4mscompany.com/SMR/manual/SMR-manual-1.1.1.pdf | `.ewa-artifacts/4ms-ambient/manuals/SMR-manual-1.1.1.pdf` | Official v1.1.1 manual for firmware v5, February 2017. The manual says six resonator/filter channels; the epic asks for an eight-band VibeLang implementation, so that should be treated as a deliberate extension. |
| Dual Looping Delay (DLD) | https://4mscompany.com/dld.php | https://4mscompany.com/DLD/manual/DLD-manual-1.1c-v5.pdf | `.ewa-artifacts/4ms-ambient/manuals/DLD-manual-1.1c-v5.pdf` | Official v1.1c manual for firmware version 5, January 11, 2017. |
| Phaseur / Phaseur Fleur | https://4mscompany.com/phaseurpedal.php and https://4mscompany.com/phaseur-kit.php?c=21 | No current Eurorack PDF manual found. Legacy documentation entry: https://4mscompany.com/phaseur-kit.php?c=21 links to Parts List/Diagrams at commonsound. | Not cached as a PDF. | Phaseur is not listed as a current 4ms Eurorack module. The official 4ms pages describe the discontinued Phaseur Fleur pedal/kit with controls Speed, Depth, Height, Blend, and Ring. Use this as behavioral inspiration, not as a Eurorack module manual. |
| Pingable Envelope Generator (PEG) | https://4mscompany.com/peg.php | https://www.4ms.info/peg/manuals/PEG_manual_v4.3.pdf | `.ewa-artifacts/4ms-ambient/manuals/PEG_manual_v4.3.pdf` | Official firmware hub links this as "User Manual for PCB v2, firmware v4.3"; the PDF title is 4ms Pingable Envelope Generator, manual v2015-06-15. |
| Stereo Triggered Sampler (STS) | https://4mscompany.com/sts.php | https://4mscompany.com/media/STS/manual/STS-manual-1.5.pdf | `.ewa-artifacts/4ms-ambient/manuals/STS-manual-1.5.pdf` | Official user manual 1.1, March 7, 2022, for firmware v1.5. |
| Tapographic Delay (TD/TAPO) | https://4mscompany.com/tapo.php | https://www.4ms.info/TAPO/Tapographic-Delay-User-Manual-1_0.pdf | `.ewa-artifacts/4ms-ambient/manuals/Tapographic-Delay-User-Manual-1_0.pdf` | Official user manual 1.0, November 3, 2017, for firmware v1.0. |
| Quad Clock Distributor (QCD) | https://4mscompany.com/qcd.php | https://4mspedals.com/QCD/QCD-manual-v2.pdf | `.ewa-artifacts/4ms-ambient/manuals/QCD-manual-v2.pdf` | Official QCD product page links this as "QCD User Manual v2.0"; manual title is Eurorack Module User Manual v2.0, 2015-02-20. |

## Cross-Check Sources

- 4ms manuals and firmware hub: https://www.4ms.info/firmware.php
- 4ms modules catalogue: https://4mscompany.com/modules.php
- 4ms Eurorack overview: https://4mscompany.com/eurorack.php

## Manual Cache

Cached PDFs:

```text
.ewa-artifacts/4ms-ambient/manuals/DLD-manual-1.1c-v5.pdf
.ewa-artifacts/4ms-ambient/manuals/PEG_manual_v4.3.pdf
.ewa-artifacts/4ms-ambient/manuals/QCD-manual-v2.pdf
.ewa-artifacts/4ms-ambient/manuals/SMR-manual-1.1.1.pdf
.ewa-artifacts/4ms-ambient/manuals/STS-manual-1.5.pdf
.ewa-artifacts/4ms-ambient/manuals/Tapographic-Delay-User-Manual-1_0.pdf
```

Verification command used for ignore status:

```sh
git check-ignore -v .ewa-artifacts/4ms-ambient/manuals/SMR-manual-1.1.1.pdf
```

Result: ignored by `.git/info/exclude` via `.ewa-artifacts/`.
