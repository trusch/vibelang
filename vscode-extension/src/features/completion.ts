import * as vscode from 'vscode';
import { DataLoader, UGenDefinition, RhaiFunction } from '../utils/dataLoader';

export class VibelangCompletionItemProvider implements vscode.CompletionItemProvider {
    private extensionPath: string;

    constructor(extensionPath: string) {
        this.extensionPath = extensionPath;
    }

    async provideCompletionItems(
        document: vscode.TextDocument,
        position: vscode.Position,
        token: vscode.CancellationToken,
        context: vscode.CompletionContext
    ): Promise<vscode.CompletionItem[]> {
        const items: vscode.CompletionItem[] = [];

        // 1. UGen Completions
        const ugens = await DataLoader.loadUGens(this.extensionPath);
        for (const ugen of ugens) {
            // Provide completions for both _ar (audio rate) and _kr (control rate)
            // Or rely on the 'rates' field in UGenDefinition
            
            for (const rate of ugen.rates) {
                if (rate === 'ar' || rate === 'kr') {
                    const snakeName = this.toSnakeCase(ugen.name);
                    const funcName = `${snakeName}_${rate}`;
                    
                    const item = new vscode.CompletionItem(funcName, vscode.CompletionItemKind.Function);
                    item.detail = `UGen: ${ugen.name} (${rate.toUpperCase()})`;
                    item.documentation = new vscode.MarkdownString(this.formatUGenDocs(ugen));
                    item.insertText = new vscode.SnippetString(`${funcName}(${this.formatUGenSnippet(ugen)})`);
                    items.push(item);
                }
            }
            
            // Also add the class name itself? Maybe not if we only use functional API.
        }

        // 2. Rhai API Completions
        const rhaiApi = DataLoader.loadRhaiApi(this.extensionPath);
        for (const func of rhaiApi) {
            const item = new vscode.CompletionItem(func.name, vscode.CompletionItemKind.Function);
            item.detail = "Vibelang API";
            item.documentation = new vscode.MarkdownString(`**${func.signature}**\n\n${func.description}\n\n*Example:*\n\`\`\`vibe\n${func.example}\n\`\`\``);
            // Simple snippet generation based on signature
            const snippet = this.generateSnippetFromSignature(func.signature);
            if (snippet) {
                item.insertText = new vscode.SnippetString(snippet);
            }
            items.push(item);
        }

        return items;
    }

    private toSnakeCase(str: string): string {
        return str.replace(/[A-Z]/g, letter => `_${letter.toLowerCase()}`).replace(/^_/, "");
    }

    private formatUGenDocs(ugen: UGenDefinition): string {
        let docs = `**${ugen.description}**\n\n`;
        docs += `*Inputs:*\n`;
        for (const input of ugen.inputs) {
            docs += `- \`${input.name}\` (${input.type}): ${input.description} (default: ${input.default})\n`;
        }
        return docs;
    }

    private formatUGenSnippet(ugen: UGenDefinition): string {
        // Generates snippet like: ${1:freq}, ${2:phase}
        return ugen.inputs.map((input, index) => `\${${index+1}:${input.name}}`).join(', ');
    }

    private generateSnippetFromSignature(signature: string): string | null {
        // define_synthdef(name: string, body: fn) -> builder-style snippet
        if (signature.startsWith('define_synthdef')) {
            return 'define_synthdef("${1:name}", |builder| {\n\tbuilder\n\t\t.param("${2:param}", ${3:default})\n\t\t.body(|${4:param}| {\n\t\t\t$0\n\t\t})\n});';
        }
        if (signature.startsWith('define_group')) {
            return 'define_group("${1:name}", || {\n\t$0\n});';
        }
        if (signature.startsWith('pattern')) {
            return 'pattern("${1:name}")\n\t.on(${2:voice})\n\t.step("${3:step}")\n\t.start();';
        }
        if (signature.startsWith('melody')) {
            return 'melody("${1:name}")\n\t.on(${2:voice})\n\t.notes([${3:notes}])\n\t.start();';
        }
        // Generic fallback
        const match = signature.match(/([a-zA-Z0-9_]+)\((.*)\)/);
        if (match) {
            const name = match[1];
            const args = match[2].split(',').map(a => a.trim()).filter(a => a.length > 0);
            if (args.length === 0) return `${name}()`;
            
            const argSnippet = args.map((a, i) => `\${${i+1}:${a.split(':')[0].trim()}}`).join(', ');
            return `${name}(${argSnippet})`;
        }
        return null;
    }
}
