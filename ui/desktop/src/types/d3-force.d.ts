// Minimal ambient declarations for the subset of `d3-force` we use directly.
// (The package ships no bundled types and we don't want to pull in
// `@types/d3-force` just for two factory functions.)
declare module 'd3-force' {
  export interface PositioningForce {
    (alpha: number): void;
    strength(strength: number): this;
    x?(x: number): this;
    y?(y: number): this;
  }

  /// Centering force pulling nodes toward the given x coordinate.
  export function forceX(x?: number): PositioningForce;

  /// Centering force pulling nodes toward the given y coordinate.
  export function forceY(y?: number): PositioningForce;
}
