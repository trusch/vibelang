#!/usr/bin/env node
/**
 * Script to add snake_case function names to UGen manifest JSON files
 *
 * This script reads each UGen manifest and adds a "functions" array containing
 * the snake_case function names (with _ar and _kr suffixes) that can be used
 * to invoke the UGen in VibeLang code.
 */

const fs = require('fs');
const path = require('path');

// Path to UGen manifests directory
const MANIFESTS_PATH = path.resolve(__dirname, '../../crates/vibelang-dsp/ugen_manifests');

/**
 * Convert PascalCase/CamelCase to snake_case
 * Handles acronyms properly: HPF -> hpf, LFSaw -> lf_saw, SinOsc -> sin_osc
 */
function toSnakeCase(str) {
    let result = '';
    for (let i = 0; i < str.length; i++) {
        const char = str[i];
        const isUpper = char >= 'A' && char <= 'Z';
        const nextChar = str[i + 1];
        const nextIsLower = nextChar && nextChar >= 'a' && nextChar <= 'z';
        const prevChar = str[i - 1];
        const prevIsUpper = prevChar && prevChar >= 'A' && prevChar <= 'Z';

        if (isUpper && i > 0 && !prevIsUpper) {
            // New word starting with uppercase after lowercase
            result += '_' + char.toLowerCase();
        } else if (isUpper && prevIsUpper && nextIsLower) {
            // End of acronym (e.g., the 'S' in 'LFSaw')
            result += '_' + char.toLowerCase();
        } else {
            result += char.toLowerCase();
        }
    }
    return result;
}

/**
 * Generate the function names for a UGen based on its rates
 */
function generateFunctionNames(name, rates) {
    const snake = toSnakeCase(name);
    const functions = [];

    if (!rates || rates.length === 0) {
        // Default to ar/kr if no rates specified
        functions.push(snake + '_ar', snake + '_kr');
    } else if (rates.includes('builder')) {
        // Builder types use the bare name
        functions.push(snake);
    } else {
        // Standard UGens with rate suffixes
        for (const rate of rates) {
            if (rate === 'ar' || rate === 'kr') {
                functions.push(snake + '_' + rate);
            }
        }
    }

    return functions;
}

/**
 * Process a single manifest file
 */
function processManifestFile(filePath) {
    const content = fs.readFileSync(filePath, 'utf-8');
    let ugens;

    try {
        ugens = JSON.parse(content);
    } catch (e) {
        console.error(`Error parsing ${filePath}: ${e.message}`);
        return false;
    }

    let modified = false;

    for (const ugen of ugens) {
        const functions = generateFunctionNames(ugen.name, ugen.rates);

        // Only update if functions array is different or missing
        if (!ugen.functions || JSON.stringify(ugen.functions) !== JSON.stringify(functions)) {
            ugen.functions = functions;
            modified = true;
        }
    }

    if (modified) {
        // Write back with pretty formatting
        fs.writeFileSync(filePath, JSON.stringify(ugens, null, 2) + '\n');
        console.log(`Updated: ${path.basename(filePath)}`);
    } else {
        console.log(`No changes: ${path.basename(filePath)}`);
    }

    return modified;
}

/**
 * Main function
 */
function main() {
    console.log('Adding snake_case function names to UGen manifests...\n');

    if (!fs.existsSync(MANIFESTS_PATH)) {
        console.error(`Manifests directory not found: ${MANIFESTS_PATH}`);
        process.exit(1);
    }

    const files = fs.readdirSync(MANIFESTS_PATH)
        .filter(f => f.endsWith('.json'))
        .map(f => path.join(MANIFESTS_PATH, f));

    let updatedCount = 0;

    for (const file of files) {
        if (processManifestFile(file)) {
            updatedCount++;
        }
    }

    console.log(`\nDone! Updated ${updatedCount} of ${files.length} files.`);

    // Print a few examples for verification
    console.log('\nExamples:');
    const examples = ['HPF', 'LFSaw', 'SinOsc', 'RLPF', 'BPeakEQ', 'MoogFF'];
    for (const name of examples) {
        console.log(`  ${name} -> ${toSnakeCase(name)}`);
    }
}

main();
