declare global {
  type GlobalScript = () => boolean;
  type ScriptContext = { [key: string]: unknown };

  interface RunnableOptions<TContext extends ScriptContext = ScriptContext> {
    setup?: (ctx: TContext) => void;
    preAction?: (ctx: TContext) => void;
    postAction?: (ctx: TContext) => void;
    cleanup?: (ctx: TContext) => void;
    forceTown?: boolean;
    bossid?: number;
    startArea?: number;
  }

  // @ts-ignore
  class Runnable<TContext extends ScriptContext = ScriptContext> {
    constructor(action: (ctx: TContext) => boolean, options: Partial<RunnableOptions<TContext>>);

    action: (ctx: TContext) => boolean;
    startArea: number | null;
    setup: ((ctx: TContext) => void) | null;
    preAction: (ctx: TContext) => void;
    postAction: ((ctx: TContext) => void) | null;
    cleanup: ((ctx: TContext) => void) | null;
    forceTown: boolean;
    bossid: number | null;
  }

  namespace Loader {
    const fileList: string[];
    const scriptList: string[];
    const scriptIndex: number;
    const skipTown: string[];
    const firstScriptAct: number;
    const currentScript: GlobalScript | Runnable | null;
    const nextScript: GlobalScript | Runnable | null;
    const doneScripts: Set<string>;
    const tempList: string[];

    function init(): void;
    function getScripts(): void;
    function _runCurrent(ctx: ScriptContext): boolean;
    function clone(obj: any): void;
    function copy(from: any, to: any): void;
    function loadScripts(): void;
    function runScript(name: string, configOverride: Partial<IConfig> | (() => void)): boolean;
    function scriptName(offset?: number): string;
  }

  type Scripts = {
    [key in KolbotScript]: boolean;
  };

  const Scripts: Scripts;
}
export {};
