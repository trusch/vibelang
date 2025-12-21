import * as vscode from 'vscode';
import { DataLoader, UGenDefinition, RhaiFunction, StdlibItem } from '../utils/dataLoader';

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

        // 0. Rhai Language Keywords & Loop Completions
        items.push(...this.getRhaiKeywordCompletions());

        // 1. UGen Completions
        const ugens = await DataLoader.loadUGens(this.extensionPath);
        for (const ugen of ugens) {
            // Use the pre-computed functions array for accurate function names
            const functions = ugen.functions || [];
            const isBuilder = ugen.rates?.includes('builder');

            for (const funcName of functions) {
                const item = new vscode.CompletionItem(funcName, vscode.CompletionItemKind.Function);

                if (isBuilder) {
                    item.detail = `Builder: ${ugen.name}`;
                    item.documentation = new vscode.MarkdownString(this.formatBuilderDocs(ugen));
                    item.insertText = new vscode.SnippetString(`${funcName}()$0`);
                } else {
                    // Determine rate from function name
                    const rate = funcName.endsWith('_ar') ? 'AR' : funcName.endsWith('_kr') ? 'KR' : '';
                    item.detail = `UGen: ${ugen.name}${rate ? ` (${rate})` : ''}`;
                    item.documentation = new vscode.MarkdownString(this.formatUGenDocs(ugen));
                    item.insertText = new vscode.SnippetString(`${funcName}(${this.formatUGenSnippet(ugen)})`);
                }
                items.push(item);
            }
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

        // 3. Standard Library Completions (instruments, effects, utilities)
        const stdlib = DataLoader.loadStdlib(this.extensionPath);
        for (const stdItem of stdlib) {
            const item = new vscode.CompletionItem(stdItem.name, this.getStdlibItemKind(stdItem.type));
            item.detail = `${this.capitalizeFirst(stdItem.type)} - ${stdItem.category}`;
            item.documentation = new vscode.MarkdownString(this.formatStdlibDocs(stdItem));
            // Insert as string for use with .on() or .synth()
            item.insertText = `"${stdItem.name}"`;
            items.push(item);
        }

        return items;
    }

    private getStdlibItemKind(type: string): vscode.CompletionItemKind {
        switch (type) {
            case 'instrument': return vscode.CompletionItemKind.Value;
            case 'effect': return vscode.CompletionItemKind.Module;
            case 'utility': return vscode.CompletionItemKind.Constant;
            default: return vscode.CompletionItemKind.Text;
        }
    }

    private capitalizeFirst(str: string): string {
        return str.charAt(0).toUpperCase() + str.slice(1);
    }

    private formatStdlibDocs(item: StdlibItem): string {
        let docs = `**${item.name}**\n\n${item.description}\n\n`;
        if (item.parameters && item.parameters.length > 0) {
            docs += `*Parameters:*\n`;
            for (const param of item.parameters) {
                docs += `- \`${param.name}\` (${param.type}): ${param.description} (default: ${param.default})\n`;
            }
            docs += '\n';
        }
        if (item.example) {
            docs += `*Example:*\n\`\`\`vibe\n${item.example}\n\`\`\``;
        }
        return docs;
    }

    private formatUGenDocs(ugen: UGenDefinition): string {
        let docs = `**${ugen.description}**\n\n`;
        docs += `*Inputs:*\n`;
        for (const input of ugen.inputs) {
            docs += `- \`${input.name}\` (${input.type}): ${input.description} (default: ${input.default})\n`;
        }
        return docs;
    }

    private formatBuilderDocs(ugen: UGenDefinition): string {
        let docs = `**${ugen.description}**\n\n`;
        if (ugen.inputs && ugen.inputs.length > 0) {
            docs += `*Methods:*\n`;
            for (const input of ugen.inputs) {
                if (input.type === 'method') {
                    docs += `- \`${input.name}\`: ${input.description}\n`;
                } else {
                    docs += `- \`${input.name}\` (${input.type}): ${input.description} (default: ${input.default})\n`;
                }
            }
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

    private getRhaiKeywordCompletions(): vscode.CompletionItem[] {
        const items: vscode.CompletionItem[] = [];

        // For loop
        const forItem = new vscode.CompletionItem('for', vscode.CompletionItemKind.Keyword);
        forItem.detail = 'Rhai for loop';
        forItem.documentation = new vscode.MarkdownString('Iterate over a collection.\n\n```rhai\nfor item in collection {\n    // body\n}\n```');
        forItem.insertText = new vscode.SnippetString('for ${1:item} in ${2:collection} {\n\t$0\n}');
        items.push(forItem);

        // For loop with counter
        const forCounterItem = new vscode.CompletionItem('for (with counter)', vscode.CompletionItemKind.Keyword);
        forCounterItem.detail = 'Rhai for loop with counter';
        forCounterItem.documentation = new vscode.MarkdownString('Iterate over a collection with an index counter.\n\n```rhai\nfor (item, index) in collection {\n    // body\n}\n```');
        forCounterItem.insertText = new vscode.SnippetString('for (${1:item}, ${2:i}) in ${3:collection} {\n\t$0\n}');
        forCounterItem.filterText = 'for';
        items.push(forCounterItem);

        // While loop
        const whileItem = new vscode.CompletionItem('while', vscode.CompletionItemKind.Keyword);
        whileItem.detail = 'Rhai while loop';
        whileItem.documentation = new vscode.MarkdownString('Loop while a condition is true.\n\n```rhai\nwhile condition {\n    // body\n}\n```');
        whileItem.insertText = new vscode.SnippetString('while ${1:condition} {\n\t$0\n}');
        items.push(whileItem);

        // Infinite loop
        const loopItem = new vscode.CompletionItem('loop', vscode.CompletionItemKind.Keyword);
        loopItem.detail = 'Rhai infinite loop';
        loopItem.documentation = new vscode.MarkdownString('Infinite loop (use `break` to exit).\n\n```rhai\nloop {\n    if condition {\n        break;\n    }\n}\n```');
        loopItem.insertText = new vscode.SnippetString('loop {\n\t$0\n}');
        items.push(loopItem);

        // Do-while loop
        const doWhileItem = new vscode.CompletionItem('do while', vscode.CompletionItemKind.Keyword);
        doWhileItem.detail = 'Rhai do-while loop';
        doWhileItem.documentation = new vscode.MarkdownString('Execute body at least once, then loop while condition is true.\n\n```rhai\ndo {\n    // body\n} while condition;\n```');
        doWhileItem.insertText = new vscode.SnippetString('do {\n\t$0\n} while ${1:condition};');
        doWhileItem.filterText = 'do';
        items.push(doWhileItem);

        // Do-until loop
        const doUntilItem = new vscode.CompletionItem('do until', vscode.CompletionItemKind.Keyword);
        doUntilItem.detail = 'Rhai do-until loop';
        doUntilItem.documentation = new vscode.MarkdownString('Execute body at least once, then loop until condition becomes true.\n\n```rhai\ndo {\n    // body\n} until condition;\n```');
        doUntilItem.insertText = new vscode.SnippetString('do {\n\t$0\n} until ${1:condition};');
        doUntilItem.filterText = 'do';
        items.push(doUntilItem);

        // Break statement
        const breakItem = new vscode.CompletionItem('break', vscode.CompletionItemKind.Keyword);
        breakItem.detail = 'Rhai break statement';
        breakItem.documentation = new vscode.MarkdownString('Exit a loop. Can optionally return a value.\n\n```rhai\nbreak;\nbreak value;\n```');
        breakItem.insertText = new vscode.SnippetString('break$0;');
        items.push(breakItem);

        // Continue statement
        const continueItem = new vscode.CompletionItem('continue', vscode.CompletionItemKind.Keyword);
        continueItem.detail = 'Rhai continue statement';
        continueItem.documentation = new vscode.MarkdownString('Skip to the next iteration of the loop.\n\n```rhai\ncontinue;\n```');
        continueItem.insertText = new vscode.SnippetString('continue;');
        items.push(continueItem);

        // If statement
        const ifItem = new vscode.CompletionItem('if', vscode.CompletionItemKind.Keyword);
        ifItem.detail = 'Rhai if statement';
        ifItem.documentation = new vscode.MarkdownString('Conditional execution.\n\n```rhai\nif condition {\n    // body\n}\n```');
        ifItem.insertText = new vscode.SnippetString('if ${1:condition} {\n\t$0\n}');
        items.push(ifItem);

        // If-else statement
        const ifElseItem = new vscode.CompletionItem('if else', vscode.CompletionItemKind.Keyword);
        ifElseItem.detail = 'Rhai if-else statement';
        ifElseItem.documentation = new vscode.MarkdownString('Conditional execution with else branch.\n\n```rhai\nif condition {\n    // true branch\n} else {\n    // false branch\n}\n```');
        ifElseItem.insertText = new vscode.SnippetString('if ${1:condition} {\n\t$2\n} else {\n\t$0\n}');
        ifElseItem.filterText = 'if';
        items.push(ifElseItem);

        // Let binding
        const letItem = new vscode.CompletionItem('let', vscode.CompletionItemKind.Keyword);
        letItem.detail = 'Rhai variable declaration';
        letItem.documentation = new vscode.MarkdownString('Declare a mutable variable.\n\n```rhai\nlet x = value;\n```');
        letItem.insertText = new vscode.SnippetString('let ${1:name} = ${0};');
        items.push(letItem);

        // Const binding
        const constItem = new vscode.CompletionItem('const', vscode.CompletionItemKind.Keyword);
        constItem.detail = 'Rhai constant declaration';
        constItem.documentation = new vscode.MarkdownString('Declare an immutable constant.\n\n```rhai\nconst X = value;\n```');
        constItem.insertText = new vscode.SnippetString('const ${1:NAME} = ${0};');
        items.push(constItem);

        // Function definition
        const fnItem = new vscode.CompletionItem('fn', vscode.CompletionItemKind.Keyword);
        fnItem.detail = 'Rhai function definition';
        fnItem.documentation = new vscode.MarkdownString('Define a function.\n\n```rhai\nfn name(param1, param2) {\n    // body\n}\n```');
        fnItem.insertText = new vscode.SnippetString('fn ${1:name}(${2:params}) {\n\t$0\n}');
        items.push(fnItem);

        // Return statement
        const returnItem = new vscode.CompletionItem('return', vscode.CompletionItemKind.Keyword);
        returnItem.detail = 'Rhai return statement';
        returnItem.documentation = new vscode.MarkdownString('Return a value from a function.\n\n```rhai\nreturn value;\n```');
        returnItem.insertText = new vscode.SnippetString('return $0;');
        items.push(returnItem);

        return items;
    }
}
