/// <reference path="../../../sdk/globals.d.ts" />

declare global {
  namespace TorchSystem {
    interface FarmerProfile {
      KeyFinderProfiles: string[];
      FarmGame: string;
      profile?: string;
    }

    let FarmerProfiles: { [key: string]: FarmerProfile };
    let inGame: boolean;
    let check: boolean;

    function getFarmers(): FarmerProfile[] | false;
    function isFarmer(): FarmerProfile | false;
    function inGameCheck(): boolean;
    function keyCheck(): number[];
    function outOfGameCheck(): boolean;
    function waitForKeys(): void;
  }
}
export {};