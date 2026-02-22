declare global {
  const Common: {
    load: (moduleName: string) => unknown;

    Ancients: typeof import("../../libs/core/Common/Ancients");
    Baal: typeof import("../../libs/core/Common/Baal");
    Cain: typeof import("../../libs/core/Common/Cain");
    Cows: typeof import("../../libs/core/Common/Cows");
    Diablo: typeof import("../../libs/core/Common/Diablo") & EventsInstance;
    Leecher: typeof import("../../libs/core/Common/Leecher");
    Smith: typeof import("../../libs/core/Common/Smith");
    Toolsthread: typeof import("../../libs/core/Common/Tools");
  };
}
export {};
