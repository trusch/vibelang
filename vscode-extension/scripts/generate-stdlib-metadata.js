#!/usr/bin/env node
/**
 * Script to generate stdlib metadata JSON from .vibe files
 *
 * This parses the stdlib directory and extracts synthdef definitions,
 * including names, parameters, and descriptions.
 */

const fs = require('fs');
const path = require('path');

// Path to stdlib directory (relative to vscode-extension)
const STDLIB_PATH = path.resolve(__dirname, '../../crates/vibelang-std/stdlib');
const OUTPUT_PATH = path.resolve(__dirname, '../src/data/stdlib.json');

/**
 * Recursively find all .vibe files in a directory
 */
function findVibeFiles(dir, files = []) {
    const entries = fs.readdirSync(dir, { withFileTypes: true });

    for (const entry of entries) {
        const fullPath = path.join(dir, entry.name);
        if (entry.isDirectory()) {
            findVibeFiles(fullPath, files);
        } else if (entry.name.endsWith('.vibe')) {
            files.push(fullPath);
        }
    }

    return files;
}

/**
 * Parse a .vibe file and extract synthdef and effect definitions
 */
function parseVibeFile(filePath) {
    const content = fs.readFileSync(filePath, 'utf-8');
    const synthdefs = [];

    // Extract description from first line comment
    const descriptionMatch = content.match(/^\/\/\s*(.+)$/m);
    const fileDescription = descriptionMatch ? descriptionMatch[1].trim() : null;

    // Calculate relative path from stdlib root
    const relativePath = path.relative(STDLIB_PATH, filePath).replace(/\\/g, '/');
    const importPath = `stdlib/${relativePath}`;

    // Determine category from path
    const pathParts = relativePath.split('/');
    const category = pathParts[0]; // e.g., "bass", "leads", "drums", "effects"
    const subcategory = pathParts.length > 2 ? pathParts[1] : null;

    // Match define_synthdef calls (instruments)
    const synthdefRegex = /define_synthdef\s*\(\s*"([^"]+)"\s*,\s*\|builder\|\s*\{([\s\S]*?)\}\s*\)\s*;/g;

    let match;
    while ((match = synthdefRegex.exec(content)) !== null) {
        const name = match[1];
        const builderBody = match[2];
        const params = extractParams(builderBody);

        synthdefs.push({
            name,
            type: 'instrument',
            description: fileDescription,
            category,
            subcategory,
            importPath,
            sourcePath: relativePath,
            params
        });
    }

    // Match define_fx calls (effects) - builder style: define_fx("name", |builder| { builder.param(...).body(...) })
    const fxBuilderRegex = /define_fx\s*\(\s*"([^"]+)"\s*,\s*\|builder\|\s*\{([\s\S]*?)\}\s*\)\s*;/g;

    while ((match = fxBuilderRegex.exec(content)) !== null) {
        const name = match[1];
        const builderBody = match[2];
        const params = extractParams(builderBody);

        synthdefs.push({
            name,
            type: 'effect',
            description: fileDescription,
            category,
            subcategory,
            importPath,
            sourcePath: relativePath,
            params
        });
    }

    // Match define_fx calls (effects) - chained style: define_fx("name").param(...).body(...)
    const fxChainedRegex = /define_fx\s*\(\s*"([^"]+)"\s*\)([\s\S]*?)\.body\s*\(/g;

    while ((match = fxChainedRegex.exec(content)) !== null) {
        const name = match[1];
        const chainedMethods = match[2];
        const params = extractParams(chainedMethods);

        synthdefs.push({
            name,
            type: 'effect',
            description: fileDescription,
            category,
            subcategory,
            importPath,
            sourcePath: relativePath,
            params
        });
    }

    return synthdefs;
}

/**
 * Extract parameters from builder body or chained methods
 */
function extractParams(code) {
    const params = [];
    const paramRegex = /\.param\s*\(\s*"([^"]+)"\s*,\s*([^)]+)\)/g;

    let paramMatch;
    while ((paramMatch = paramRegex.exec(code)) !== null) {
        const paramName = paramMatch[1];
        const defaultValue = paramMatch[2].trim();

        // Try to parse numeric value
        let parsedValue = parseFloat(defaultValue);
        if (isNaN(parsedValue)) {
            parsedValue = defaultValue;
        }

        params.push({
            name: paramName,
            default: parsedValue
        });
    }

    return params;
}

/**
 * Main function
 */
function main() {
    console.log(`Scanning stdlib at: ${STDLIB_PATH}`);

    if (!fs.existsSync(STDLIB_PATH)) {
        console.error(`Error: stdlib directory not found at ${STDLIB_PATH}`);
        process.exit(1);
    }

    const vibeFiles = findVibeFiles(STDLIB_PATH);
    console.log(`Found ${vibeFiles.length} .vibe files`);

    const allSynthdefs = [];

    for (const file of vibeFiles) {
        try {
            const synthdefs = parseVibeFile(file);
            allSynthdefs.push(...synthdefs);
        } catch (err) {
            console.warn(`Warning: Failed to parse ${file}: ${err.message}`);
        }
    }

    console.log(`Extracted ${allSynthdefs.length} synthdef definitions`);

    // Sort by category then name
    allSynthdefs.sort((a, b) => {
        if (a.category !== b.category) {
            return a.category.localeCompare(b.category);
        }
        if (a.subcategory !== b.subcategory) {
            return (a.subcategory || '').localeCompare(b.subcategory || '');
        }
        return a.name.localeCompare(b.name);
    });

    // Build output structure
    const output = {
        version: new Date().toISOString(),
        synthdefs: allSynthdefs,
        categories: [...new Set(allSynthdefs.map(s => s.category))].sort()
    };

    // Ensure output directory exists
    const outputDir = path.dirname(OUTPUT_PATH);
    if (!fs.existsSync(outputDir)) {
        fs.mkdirSync(outputDir, { recursive: true });
    }

    // Write output
    fs.writeFileSync(OUTPUT_PATH, JSON.stringify(output, null, 2));
    console.log(`Written to: ${OUTPUT_PATH}`);

    // Print summary by category
    console.log('\nSummary by category:');
    const byCat = {};
    for (const s of allSynthdefs) {
        byCat[s.category] = (byCat[s.category] || 0) + 1;
    }
    for (const [cat, count] of Object.entries(byCat).sort()) {
        console.log(`  ${cat}: ${count}`);
    }
}

main();
