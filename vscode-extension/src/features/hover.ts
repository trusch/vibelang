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
        const range = document.getWordRangeAtPosition(position);
        if (!range) return null;

        const word = document.getText(range);

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
        // We need to handle the mapping from sin_ar -> SinOsc
        // Or just search inputs/outputs/description if we can't match name exactly.
        // The completion provider mapped SinOsc -> sin_ar. We should do the reverse or search.
        const ugens = await DataLoader.loadUGens(this.extensionPath);
        const matchedUGen = ugens.find(u => {
             const snake = this.toSnakeCase(u.name);
             return snake + '_ar' === word || snake + '_kr' === word || u.name === word;
        });

        if (matchedUGen) {
             const markdown = new vscode.MarkdownString();
             markdown.appendMarkdown(`**UGen: ${matchedUGen.name}**\n\n`);
             markdown.appendMarkdown(`${matchedUGen.description}\n\n`);
             markdown.appendMarkdown(`**Inputs:**\n`);
             matchedUGen.inputs.forEach(inp => {
                 markdown.appendMarkdown(`- \`${inp.name}\` (${inp.type}): ${inp.description} (default: ${inp.default})\n`);
             });
             markdown.appendMarkdown(`\n**Outputs:** ${matchedUGen.outputs}`);
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

    private toSnakeCase(str: string): string {
        return str.replace(/[A-Z]/g, letter => `_${letter.toLowerCase()}`).replace(/^_/, "");
    }
}
