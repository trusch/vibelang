import { useState, useCallback, useRef } from 'react';

export function useSoundPreview(vibelangRef) {
  const [isPlaying, setIsPlaying] = useState(false);
  const [currentPreview, setCurrentPreview] = useState(null);
  const timeoutRef = useRef(null);

  const previewSynthdef = useCallback(async (synthdef) => {
    if (!vibelangRef?.current) return;

    // Stop any current preview
    if (timeoutRef.current) {
      clearTimeout(timeoutRef.current);
    }

    setIsPlaying(true);
    setCurrentPreview(synthdef.name);

    try {
      // Initialize if not ready
      if (!vibelangRef.current.isReady()) {
        await vibelangRef.current.initVibelang({ enableAudio: true });
      }

      // Generate preview script based on synthdef type
      const previewScript = generatePreviewScript(synthdef);

      // Execute the preview
      await vibelangRef.current.executeScript(previewScript);

      // Auto-stop after 3 seconds
      timeoutRef.current = setTimeout(() => {
        stopPreview();
      }, 3000);

    } catch (err) {
      console.error('Preview error:', err);
      setIsPlaying(false);
      setCurrentPreview(null);
    }
  }, [vibelangRef]);

  const stopPreview = useCallback(async () => {
    if (timeoutRef.current) {
      clearTimeout(timeoutRef.current);
    }

    if (vibelangRef?.current) {
      try {
        await vibelangRef.current.stopAll();
      } catch (err) {
        console.error('Stop preview error:', err);
      }
    }

    setIsPlaying(false);
    setCurrentPreview(null);
  }, [vibelangRef]);

  return {
    isPlaying,
    currentPreview,
    previewSynthdef,
    stopPreview
  };
}

function generatePreviewScript(synthdef) {
  const { name, source, category, subcategory } = synthdef;

  // Determine if this is a melodic or percussive sound
  // Check both category and subcategory for better detection
  const categoryLower = category?.toLowerCase() || '';
  const subcategoryLower = subcategory?.toLowerCase() || '';
  const nameLower = name?.toLowerCase() || '';

  const isPercussive = categoryLower.includes('drum') ||
                       subcategoryLower.includes('kick') ||
                       subcategoryLower.includes('snare') ||
                       subcategoryLower.includes('hat') ||
                       subcategoryLower.includes('clap') ||
                       subcategoryLower.includes('perc') ||
                       subcategoryLower.includes('cymbal') ||
                       nameLower.includes('kick') ||
                       nameLower.includes('snare') ||
                       nameLower.includes('hihat') ||
                       nameLower.includes('clap');

  const isBass = categoryLower.includes('bass');
  const isLead = categoryLower.includes('lead') ||
                 categoryLower.includes('synth');
  const isPad = categoryLower.includes('pad') ||
                categoryLower.includes('texture');

  // Use inline source code instead of imports (WASM doesn't support file imports)
  let script = `set_tempo(120);\n\n`;
  script += `// ${name}\n`;
  script += `${source || ''}\n\n`;

  if (isPercussive) {
    // Trigger a few hits for percussive sounds
    script += `
define_group("Preview", || {
    let preview = voice("preview").synth("${name}").gain(db(-6));

    // Trigger a few times
    preview.trigger();
    sleep("250ms");
    preview.trigger();
    sleep("250ms");
    preview.trigger();
    sleep("250ms");
    preview.trigger();
});
`;
  } else if (isBass) {
    // Play a simple bass note
    script += `
define_group("Preview", || {
    let preview = voice("preview").synth("${name}").gain(db(-6));

    melody("preview_melody")
        .on(preview)
        .notes("E2 - - . | G2 - - .")
        .start();
});
`;
  } else if (isPad) {
    // Play a sustained chord
    script += `
define_group("Preview", || {
    let preview = voice("preview").synth("${name}").poly(3).gain(db(-9));

    // Play a chord
    preview.note_on(60, 0.7);
    preview.note_on(64, 0.6);
    preview.note_on(67, 0.6);
});
`;
  } else {
    // Default: play a short melody
    script += `
define_group("Preview", || {
    let preview = voice("preview").synth("${name}").gain(db(-6));

    melody("preview_melody")
        .on(preview)
        .notes("C4 E4 G4 C5")
        .gate(0.5)
        .start();
});
`;
  }

  return script;
}

export default useSoundPreview;
