import * as vscode from 'vscode';
import { DataLoader, UGenDefinition, RhaiFunction, StdlibItem } from '../utils/dataLoader';

export class VibelangHoverProvider implements vscode.HoverProvider {
    private extensionPath: string;

    constructor(extensionPath: string) {
        this.extensionPath = extensionPath;
    }

    async provideHover(
        document: vscode.TextDocument,
        position: vscode.Position,
        token: vscode.CancellationToken
    ): Promise<vscode.Hover | null> {
        // Use custom word pattern that includes underscores for snake_case identifiers
        const wordPattern = /[a-zA-Z_][a-zA-Z0-9_]*/;
        const range = document.getWordRangeAtPosition(position, wordPattern);
        if (!range) return null;

        const word = document.getText(range);

        // 0. Check Rhai Keywords
        const keywordHover = this.getRhaiKeywordHover(word);
        if (keywordHover) {
            return keywordHover;
        }

        // 1. Check Rhai API
        const rhaiApi = DataLoader.loadRhaiApi(this.extensionPath);
        const func = rhaiApi.find(f => f.name === word);
        if (func) {
            const markdown = new vscode.MarkdownString();
            markdown.appendCodeblock(func.signature, 'vibe');
            markdown.appendMarkdown(`\n\n${func.description}\n\n`);
            if (func.example) {
                markdown.appendMarkdown(`**Example:**\n\`\`\`vibe\n${func.example}\n\`\`\``);
            }
            return new vscode.Hover(markdown);
        }

        // 2. Check UGens
        // Each UGen has a "functions" array with the actual function names (e.g., ["hpf_ar", "hpf_kr"])
        const ugens = await DataLoader.loadUGens(this.extensionPath);
        const matchedUGen = ugens.find(u => {
             // Check if word matches any of the function names in the UGen's functions array
             if (u.functions && u.functions.includes(word)) {
                 return true;
             }
             // Also check the original name (e.g., "envelope" for builder types)
             if (u.name === word) {
                 return true;
             }
             return false;
        });

        if (matchedUGen) {
             const markdown = new vscode.MarkdownString();
             const isBuilder = matchedUGen.rates?.includes('builder');
             markdown.appendMarkdown(`**${isBuilder ? 'Builder' : 'UGen'}: ${matchedUGen.name}**\n\n`);
             markdown.appendMarkdown(`${matchedUGen.description}\n\n`);

             if (matchedUGen.inputs && matchedUGen.inputs.length > 0) {
                 markdown.appendMarkdown(`**${isBuilder ? 'Methods' : 'Inputs'}:**\n`);
                 matchedUGen.inputs.forEach(inp => {
                     if (isBuilder && inp.type === 'method') {
                         // For builder methods, show cleaner format
                         markdown.appendMarkdown(`- \`${inp.name}\`: ${inp.description}\n`);
                     } else {
                         markdown.appendMarkdown(`- \`${inp.name}\` (${inp.type}): ${inp.description} (default: ${inp.default})\n`);
                     }
                 });
             }

             if (!isBuilder) {
                 markdown.appendMarkdown(`\n**Outputs:** ${matchedUGen.outputs}`);
             }
             return new vscode.Hover(markdown);
        }

        // 3. Check Standard Library (instruments, effects, utilities)
        // Check both with and without quotes since user might hover over string content
        const cleanWord = word.replace(/^["']|["']$/g, ''); // Remove surrounding quotes if any
        const stdlib = DataLoader.loadStdlib(this.extensionPath);
        const matchedStdlib = stdlib.find(s => s.name === word || s.name === cleanWord);

        if (matchedStdlib) {
            const markdown = new vscode.MarkdownString();
            const typeIcon = matchedStdlib.type === 'instrument' ? '🎹' :
                            matchedStdlib.type === 'effect' ? '🎛️' : '🔧';
            markdown.appendMarkdown(`${typeIcon} **${matchedStdlib.name}** (${matchedStdlib.category})\n\n`);
            markdown.appendMarkdown(`${matchedStdlib.description}\n\n`);

            if (matchedStdlib.parameters && matchedStdlib.parameters.length > 0) {
                markdown.appendMarkdown(`**Parameters:**\n`);
                matchedStdlib.parameters.forEach(param => {
                    markdown.appendMarkdown(`- \`${param.name}\` (${param.type}): ${param.description} (default: ${param.default})\n`);
                });
                markdown.appendMarkdown('\n');
            }

            if (matchedStdlib.example) {
                markdown.appendMarkdown(`**Example:**\n\`\`\`vibe\n${matchedStdlib.example}\n\`\`\``);
            }
            return new vscode.Hover(markdown);
        }

        return null;
    }

    private getRhaiKeywordHover(word: string): vscode.Hover | null {
        const keywords: Record<string, { syntax: string; description: string }> = {
            'for': {
                syntax: 'for item in collection { ... }\nfor (item, index) in collection { ... }',
                description: 'Iterate over a collection. Optionally includes a counter variable that starts at 0 and increments each iteration. The loop can return a value via `break value;`.'
            },
            'in': {
                syntax: 'for item in collection { ... }',
                description: 'Used with `for` to specify the collection to iterate over.'
            },
            'while': {
                syntax: 'while condition { ... }',
                description: 'Loop while the condition is true. Use `break` to exit early or `continue` to skip to the next iteration. Can return a value via `break value;`.'
            },
            'loop': {
                syntax: 'loop { ... }',
                description: 'Infinite loop. Must use `break` to exit. Can return a value via `break value;`.'
            },
            'do': {
                syntax: 'do { ... } while condition;\ndo { ... } until condition;',
                description: 'Execute the body at least once, then repeat while/until the condition. `do-while` continues when true, `do-until` continues when false.'
            },
            'until': {
                syntax: 'do { ... } until condition;',
                description: 'Used with `do` to create a loop that continues until the condition becomes true.'
            },
            'break': {
                syntax: 'break;\nbreak value;',
                description: 'Exit the current loop immediately. Optionally returns a value from the loop expression.'
            },
            'continue': {
                syntax: 'continue;',
                description: 'Skip the rest of the current iteration and proceed to the next iteration of the loop.'
            },
            'if': {
                syntax: 'if condition { ... }\nif condition { ... } else { ... }\nif condition { ... } else if other { ... }',
                description: 'Conditional execution. Can be used as an expression that returns a value.'
            },
            'else': {
                syntax: 'if condition { ... } else { ... }',
                description: 'The alternative branch of an `if` statement, executed when the condition is false.'
            },
            'let': {
                syntax: 'let name = value;',
                description: 'Declare a mutable variable. Variables can be reassigned after declaration.'
            },
            'const': {
                syntax: 'const NAME = value;',
                description: 'Declare an immutable constant. Constants cannot be reassigned after declaration.'
            },
            'fn': {
                syntax: 'fn name(param1, param2) { ... }',
                description: 'Define a function. Functions can return values using `return` or by having the last expression as the return value.'
            },
            'return': {
                syntax: 'return;\nreturn value;',
                description: 'Return a value from a function. If no value is specified, returns `()` (unit type).'
            },
            'import': {
                syntax: 'import "path/to/module";',
                description: 'Import another Rhai script file.'
            },
            'true': {
                syntax: 'true',
                description: 'Boolean literal representing true.'
            },
            'false': {
                syntax: 'false',
                description: 'Boolean literal representing false.'
            }
        };

        const info = keywords[word];
        if (!info) return null;

        const markdown = new vscode.MarkdownString();
        markdown.appendCodeblock(info.syntax, 'rhai');
        markdown.appendMarkdown(`\n\n${info.description}`);
        return new vscode.Hover(markdown);
    }
}
