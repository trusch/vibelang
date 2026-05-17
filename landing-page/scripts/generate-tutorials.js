#!/usr/bin/env node

/**
 * Tutorial Generator for VibeLang Playground
 *
 * Reads tutorial .vibe files from examples/tutorials/ and generates
 * the tutorials.js file for the web playground.
 *
 * The tutorials are stored as .vibe files that work with the native CLI,
 * and this script converts them for web use by transforming imports.
 *
 * Usage: node scripts/generate-tutorials.js
 */

const fs = require('fs');
const path = require('path');

const TUTORIALS_DIR = path.join(__dirname, '../../examples/tutorials');
const OUTPUT_FILE = path.join(__dirname, '../src/data/tutorials.js');

// Tutorial categories - maps tutorial ID prefixes to category names
const CATEGORY_MAP = {
  '01': 'Getting Started',
  '02': 'Getting Started',
  '03': 'Getting Started',
  '04': 'Patterns & Rhythms',
  '05': 'Patterns & Rhythms',
  '06': 'Patterns & Rhythms',
  '07': 'Melodies & Chords',
  '08': 'Melodies & Chords',
  '09': 'Melodies & Chords',
  '10': 'Mixing & Effects',
  '11': 'Mixing & Effects',
  '12': 'Arrangement',
  '13': 'Arrangement',
  '14': 'Advanced',
  '15': 'Advanced',
  '16': 'Advanced',
};

// Map file names to tutorial IDs
const ID_MAP = {
  '01_first_beat': 'first-beat',
  '02_first_melody': 'first-melody',
  '03_combining_elements': 'combining-elements',
  '04_step_patterns': 'step-patterns',
  '05_euclidean_rhythms': 'euclidean-rhythms',
  '06_polyrhythms': 'polyrhythms',
  '07_notes_scales': 'notes-scales',
  '08_chords_progressions': 'chords-progressions',
  '09_arpeggios': 'arpeggios',
  '10_groups_routing': 'groups-routing',
  '11_effects_reverb_delay': 'effects-reverb-delay',
  '12_sequences': 'sequences',
  '13_fades': 'fades',
  '14_custom_synths': 'custom-synths',
  '15_custom_effects': 'custom-effects',
  '16_cv_sources': 'cv-sources',
};

/**
 * Parse tutorial metadata from the file header comments.
 * Expected format:
 *   // Tutorial: Title Here
 *   // Difficulty: beginner | Duration: 5 min
 *   // Description: One line description.
 */
function parseMetadata(content) {
  const lines = content.split('\n');
  let title = '';
  let difficulty = 'beginner';
  let duration = '5 min';
  let description = '';

  for (const line of lines.slice(0, 5)) {
    const trimmed = line.trim();
    if (!trimmed.startsWith('//')) continue;

    const comment = trimmed.substring(2).trim();

    // Parse "Tutorial: Title"
    const titleMatch = comment.match(/^Tutorial:\s*(.+)$/i);
    if (titleMatch) {
      title = titleMatch[1].trim();
      continue;
    }

    // Parse "Difficulty: X | Duration: Y"
    const metaMatch = comment.match(/Difficulty:\s*(\w+)\s*\|\s*Duration:\s*(.+)$/i);
    if (metaMatch) {
      difficulty = metaMatch[1].trim().toLowerCase();
      duration = metaMatch[2].trim();
      continue;
    }

    // Parse "Description: ..."
    const descMatch = comment.match(/^Description:\s*(.+)$/i);
    if (descMatch) {
      description = descMatch[1].trim();
      continue;
    }
  }

  return { title, difficulty, duration, description };
}

/**
 * Keep imports in their native format for documentation purposes.
 *
 * The tutorials are displayed to users learning VibeLang, so they should
 * see the correct native CLI syntax: import "stdlib/drums/kicks/kick_909.vibe";
 *
 * The web playground component handles any necessary import transformations
 * at runtime when executing the code.
 */
function convertImportsForWeb(content) {
  // Keep imports in native format - no conversion needed
  return content;
}

/**
 * Load and parse a single tutorial file.
 */
function loadTutorial(filepath) {
  const content = fs.readFileSync(filepath, 'utf-8');
  const filename = path.basename(filepath, '.vibe');
  const prefix = filename.substring(0, 2);

  const metadata = parseMetadata(content);
  const webCode = convertImportsForWeb(content);

  return {
    id: ID_MAP[filename] || filename.replace(/_/g, '-'),
    ...metadata,
    category: CATEGORY_MAP[prefix] || 'Other',
    code: webCode,
  };
}

/**
 * Generate the tutorials.js file content.
 */
function generateTutorialsJS(tutorials) {
  // Build category structure
  const categoryTutorials = {};
  for (const tutorial of tutorials) {
    if (!categoryTutorials[tutorial.category]) {
      categoryTutorials[tutorial.category] = [];
    }
    categoryTutorials[tutorial.category].push(tutorial.id);
  }

  // Build categories array in order
  const categoryOrder = [
    'Getting Started',
    'Patterns & Rhythms',
    'Melodies & Chords',
    'Mixing & Effects',
    'Arrangement',
    'Advanced',
  ];

  const tutorialCategories = categoryOrder
    .filter(cat => categoryTutorials[cat])
    .map(cat => ({
      name: cat,
      tutorials: categoryTutorials[cat],
    }));

  // Build tutorials object
  const tutorialsObj = {};
  for (const tutorial of tutorials) {
    tutorialsObj[tutorial.id] = {
      id: tutorial.id,
      title: tutorial.title,
      description: tutorial.description,
      difficulty: tutorial.difficulty,
      duration: tutorial.duration,
      code: tutorial.code,
    };
  }

  // Generate the JS file
  const js = `// AUTO-GENERATED FILE - DO NOT EDIT DIRECTLY
// Generated by scripts/generate-tutorials.js from examples/tutorials/*.vibe
// To modify tutorials, edit the .vibe files in examples/tutorials/

export const tutorialCategories = ${JSON.stringify(tutorialCategories, null, 2)};

export const tutorials = ${JSON.stringify(tutorialsObj, null, 2)};

// Get a tutorial by ID
export function getTutorial(id) {
  return tutorials[id];
}

// Get all tutorials in a category
export function getTutorialsByCategory(categoryName) {
  const category = tutorialCategories.find(c => c.name === categoryName);
  if (!category) return [];
  return category.tutorials.map(id => tutorials[id]).filter(Boolean);
}

// Get all tutorials as flat array
export function getAllTutorials() {
  return Object.values(tutorials);
}

export default tutorials;
`;

  return js;
}

/**
 * Main function
 */
function main() {
  console.log('Generating tutorials.js from examples/tutorials/...\n');

  // Check if tutorials directory exists
  if (!fs.existsSync(TUTORIALS_DIR)) {
    console.error('Error: Tutorials directory not found:', TUTORIALS_DIR);
    process.exit(1);
  }

  // Load all tutorial files
  const files = fs.readdirSync(TUTORIALS_DIR)
    .filter(f => f.endsWith('.vibe'))
    .sort();

  console.log(`Found ${files.length} tutorial files`);

  const tutorials = [];
  for (const file of files) {
    const filepath = path.join(TUTORIALS_DIR, file);
    const tutorial = loadTutorial(filepath);
    tutorials.push(tutorial);
    console.log(`  - ${tutorial.id}: ${tutorial.title}`);
  }

  // Generate and write output
  const output = generateTutorialsJS(tutorials);

  // Ensure output directory exists
  const outputDir = path.dirname(OUTPUT_FILE);
  if (!fs.existsSync(outputDir)) {
    fs.mkdirSync(outputDir, { recursive: true });
  }

  fs.writeFileSync(OUTPUT_FILE, output);
  console.log(`\nWritten to ${OUTPUT_FILE}`);
  console.log('Done!');
}

main();
