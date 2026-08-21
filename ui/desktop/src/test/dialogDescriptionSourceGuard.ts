import ts from 'typescript';

type SurfaceKind = 'Dialog' | 'Sheet';

interface PrimitiveImports {
  contents: Map<string, SurfaceKind>;
  descriptions: Record<SurfaceKind, Set<string>>;
}

export interface UnsettledDialogSurface {
  component: 'DialogContent' | 'SheetContent';
  file: string;
  line: number;
  column: number;
}

const contentExports = new Map<string, SurfaceKind>([
  ['DialogContent', 'Dialog'],
  ['SheetContent', 'Sheet'],
]);

const descriptionExports = new Map<string, SurfaceKind>([
  ['DialogDescription', 'Dialog'],
  ['SheetDescription', 'Sheet'],
]);

function primitiveImports(sourceFile: ts.SourceFile): PrimitiveImports {
  const imports: PrimitiveImports = {
    contents: new Map(),
    descriptions: { Dialog: new Set(), Sheet: new Set() },
  };

  for (const statement of sourceFile.statements) {
    if (!ts.isImportDeclaration(statement)) continue;
    const namedImports = statement.importClause?.namedBindings;
    if (!namedImports || !ts.isNamedImports(namedImports)) continue;

    for (const specifier of namedImports.elements) {
      const exported = (specifier.propertyName ?? specifier.name).text;
      const local = specifier.name.text;
      const contentKind = contentExports.get(exported);
      if (contentKind) imports.contents.set(local, contentKind);
      const descriptionKind = descriptionExports.get(exported);
      if (descriptionKind) imports.descriptions[descriptionKind].add(local);
    }
  }

  return imports;
}

function openingElement(node: ts.Node): ts.JsxOpeningLikeElement | undefined {
  if (ts.isJsxElement(node)) return node.openingElement;
  if (ts.isJsxSelfClosingElement(node)) return node;
  return undefined;
}

function localTagName(opening: ts.JsxOpeningLikeElement): string | undefined {
  return ts.isIdentifier(opening.tagName) ? opening.tagName.text : undefined;
}

function explicitlyOptsOut(opening: ts.JsxOpeningLikeElement): boolean {
  return opening.attributes.properties.some((property) => {
    if (!ts.isJsxAttribute(property) || property.name.getText() !== 'aria-describedby')
      return false;
    const initializer = property.initializer;
    return (
      initializer !== undefined &&
      ts.isJsxExpression(initializer) &&
      initializer.expression !== undefined &&
      ts.isIdentifier(initializer.expression) &&
      initializer.expression.text === 'undefined'
    );
  });
}

function hasOwnedDescription(
  surface: ts.JsxElement | ts.JsxSelfClosingElement,
  kind: SurfaceKind,
  imports: PrimitiveImports
): boolean {
  let found = false;

  const visit = (node: ts.Node) => {
    if (found) return;
    const opening = openingElement(node);
    if (opening) {
      const tag = localTagName(opening);
      if (tag && imports.contents.has(tag)) return;
      if (tag && imports.descriptions[kind].has(tag)) {
        found = true;
        return;
      }
    }
    ts.forEachChild(node, visit);
  };

  if (ts.isJsxElement(surface)) {
    for (const child of surface.children) visit(child);
  }
  return found;
}

export function findUnsettledDialogSurfaces(
  source: string,
  file = 'source.tsx'
): UnsettledDialogSurface[] {
  const sourceFile = ts.createSourceFile(
    file,
    source,
    ts.ScriptTarget.Latest,
    true,
    ts.ScriptKind.TSX
  );
  const imports = primitiveImports(sourceFile);
  const unsettled: UnsettledDialogSurface[] = [];

  const visit = (node: ts.Node) => {
    const opening = openingElement(node);
    const tag = opening ? localTagName(opening) : undefined;
    const kind = tag ? imports.contents.get(tag) : undefined;

    if (opening && kind && !explicitlyOptsOut(opening)) {
      const surface = node as ts.JsxElement | ts.JsxSelfClosingElement;
      if (!hasOwnedDescription(surface, kind, imports)) {
        const location = sourceFile.getLineAndCharacterOfPosition(opening.getStart(sourceFile));
        unsettled.push({
          component: `${kind}Content`,
          file,
          line: location.line + 1,
          column: location.character + 1,
        });
      }
    }

    ts.forEachChild(node, visit);
  };

  visit(sourceFile);
  return unsettled;
}
