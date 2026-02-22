// @ts-nocheck
export interface GameActionType {
  LogNames: boolean;
  LogItemLevel: boolean;
  LogEquipped: boolean;
  LogMerc: boolean;
  SaveScreenShot: boolean;
  IngameTime: number;
  task: { action: string, data: any } | null;

  // Methods
  init(task: string): void;
  update(action: string, data: string | Object): void;
  gameInfo(): { gameName: string, gamePass: string };
  getLogin(): { realm: string, account: string, password: string };
  getCharacters(): string[];
  inGameCheck(): boolean;
  load(hash: string): string;
  save(hash: string, data: string): void;
  dropItems(dropList: string[]): void;
  convertLadderFiles(): void;
}

declare global {
  const GameAction: GameActionType;
}
export {};