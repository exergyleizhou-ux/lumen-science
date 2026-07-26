/**
 * Local type surface for `openchemlib` (LS5-D1-02).
 *
 * WHY THIS FILE EXISTS
 * The molecule connector (`src/main/connectors/descriptors/molecule.ts`) and the molecule/reaction
 * preview renderers are reachable from production entry points, but `openchemlib` was dropped from
 * package.json during the Open Science absorb and is not reinstated here. Every use site already
 * loads it lazily through a dynamic `import('openchemlib')` inside a try/catch, so an absent
 * package degrades to a caught render/tool error rather than a crash — but the *types* still have
 * to resolve for the surrounding code to be checked at all.
 *
 * SCOPE RULE: only the members the three consumers touch. `Molecule` and `Reaction` are declared as
 * classes (not interfaces) deliberately: consumers write `InstanceType<OclModule['Molecule']>`,
 * which requires a construct signature on the exported value.
 */
declare module 'openchemlib' {
  /** Options accepted by Molecule#toSVG; only the auto-crop pair is used by this pack. */
  export type MoleculeSvgOptions = {
    autoCrop?: boolean
    autoCropMargin?: number
    suppressChiralText?: boolean
    suppressCIPParity?: boolean
    suppressESR?: boolean
    noStereoProblem?: boolean
  }

  /** Result of getMolecularFormula(); molecule.ts reads `formula` and `relativeWeight`. */
  export type MolecularFormula = {
    formula: string
    relativeWeight: number
    absoluteWeight: number
  }

  export class Molecule {
    /** Throws on an unparseable SMILES — consumers rely on that to report `valid: false`. */
    static fromSmiles(smiles: string): Molecule
    /** Throws on an unparseable MDL molfile. */
    static fromMolfile(molfile: string): Molecule

    toSmiles(): string
    toMolfile(): string
    /** Heavy-atom count (hydrogens are implicit), reported as `heavy_atom_count`. */
    getAllAtoms(): number
    getMolecularFormula(): MolecularFormula
    toSVG(width: number, height: number, id?: string, options?: MoleculeSvgOptions): string
  }

  export class Reaction {
    /** Parses an MDL RXN document. */
    static fromRxn(rxn: string): Reaction

    /** Counts, not collections — OpenChemLib exposes indexed accessors. */
    getReactants(): number
    getProducts(): number
    getReactant(index: number): Molecule
    getProduct(index: number): Molecule
  }
}
