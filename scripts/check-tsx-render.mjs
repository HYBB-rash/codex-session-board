import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import ts from "typescript";

function returnedExpressions(body) {
    if (!body) return [];
    if (!ts.isBlock(body)) return [ts.skipParentheses(body)];
    const expressions = [];
    function visit(node) {
        if (ts.isReturnStatement(node) && node.expression) {
            expressions.push(ts.skipParentheses(node.expression));
        } else if (!ts.isFunctionLike(node)) {
            ts.forEachChild(node, visit);
        }
    }
    ts.forEachChild(body, visit);
    return expressions;
}

function vueSetup(call, checker) {
    const callee = ts.skipParentheses(call.expression);
    if (!ts.isIdentifier(callee) || !call.arguments.length) return;
    const [binding] = checker.getSymbolAtLocation(callee)?.declarations ?? [];
    if (!binding || !ts.isImportSpecifier(binding)) return;
    const importedName = (binding.propertyName ?? binding.name).text;
    if (importedName !== "defineComponent" && importedName !== "createApp") return;
    const { moduleSpecifier } = binding.parent.parent.parent;
    if (!ts.isStringLiteral(moduleSpecifier) || moduleSpecifier.text !== "vue") return;
    const options = ts.skipParentheses(call.arguments[0]);
    if (!ts.isObjectLiteralExpression(options)) return;
    const setup = options.properties.find(
        ({ name }) =>
            name && (ts.isIdentifier(name) || ts.isStringLiteral(name)) && name.text === "setup",
    );
    return setup && ts.isPropertyAssignment(setup) ? ts.skipParentheses(setup.initializer) : setup;
}

function forwardedCall(callback) {
    if (!ts.isArrowFunction(callback) && !ts.isFunctionExpression(callback)) return;
    let expression = callback.body;
    if (ts.isBlock(expression)) {
        const [statement] = expression.statements;
        if (expression.statements.length !== 1 || !ts.isReturnStatement(statement)) return;
        expression = statement.expression;
    }
    const call = expression && ts.skipParentheses(expression);
    if (call && ts.isCallExpression(call) && ts.isIdentifier(call.expression)) return call;
}

function returnsJsx(declaration) {
    const implementation = ts.isVariableDeclaration(declaration)
        ? declaration.initializer && ts.skipParentheses(declaration.initializer)
        : declaration;
    if (!implementation || !ts.isFunctionLike(implementation)) return false;
    return returnedExpressions(implementation.body).some(
        (node) =>
            ts.isJsxElement(node) || ts.isJsxSelfClosingElement(node) || ts.isJsxFragment(node),
    );
}

function exportedDeclarations(source, checker) {
    const module = checker.getSymbolAtLocation(source);
    if (!module) return new Set();
    return new Set(
        checker.getExportsOfModule(module).map((symbol) => {
            const local =
                symbol.flags & ts.SymbolFlags.Alias ? checker.getAliasedSymbol(symbol) : symbol;
            return local.valueDeclaration;
        }),
    );
}

export function findForwardedRenders(source, checker) {
    const references = new Map();
    const calls = [];
    function visit(node) {
        if (ts.isIdentifier(node)) {
            const symbol = ts.isShorthandPropertyAssignment(node.parent)
                ? checker.getShorthandAssignmentValueSymbol(node.parent)
                : checker.getSymbolAtLocation(node);
            if (symbol && symbol.valueDeclaration?.name !== node) {
                references.set(symbol, (references.get(symbol) ?? 0) + 1);
            }
        } else if (ts.isCallExpression(node)) {
            const setup = vueSetup(node, checker);
            if (setup && ts.isFunctionLike(setup)) {
                for (const expression of returnedExpressions(setup.body)) {
                    const call = forwardedCall(expression);
                    if (call) calls.push(call);
                }
            }
        }
        ts.forEachChild(node, visit);
    }
    visit(source);
    const exported = exportedDeclarations(source, checker);
    return calls.filter((call) => {
        const symbol = checker.getSymbolAtLocation(call.expression);
        const declaration = symbol?.valueDeclaration;
        return (
            declaration?.getSourceFile() === source &&
            !exported.has(declaration) &&
            references.get(symbol) === 1 &&
            returnsJsx(declaration)
        );
    });
}

function main() {
    const root = join(import.meta.dirname, "..");
    const files = ts.sys.readDirectory(join(root, "src", "ui"), [".tsx"]).sort();
    const program = ts.createProgram(files, {
        jsx: ts.JsxEmit.Preserve,
        noLib: true,
        noResolve: true,
        types: [],
    });
    const checker = program.getTypeChecker();
    for (const source of program.getSourceFiles()) {
        for (const call of findForwardedRenders(source, checker)) {
            const { line, character } = source.getLineAndCharacterOfPosition(call.getStart(source));
            const location = `${relative(root, source.fileName)}:${line + 1}:${character + 1}`;
            console.error(
                `${location}: Inline ${call.expression.text} in the render function returned by setup().`,
            );
            process.exitCode = 1;
        }
    }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) main();
