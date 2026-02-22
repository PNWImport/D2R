/**
*  @filename    OrgTorchData.js
*  @author      theBGuy
*  @desc        Data file handling for OrgTorch.js
*
*/

(function (module) {
  /**
   * @typedef {Object} OrgTorchDataObject
   * @property {string} gamename - The name of the game.
   * @property {string} gamepassword - The password for the game.
   * @property {number} active - The index of the active area.
   * @property {Array<number>} doneAreas - An array of completed areas.
   */
  
  const OrgTorchData = {
    _path: "data/" + me.profile + "/orgtorch.json",
    /** @type {OrgTorchDataObject} */
    _default: { gamename: "", gamepassword: "", active: -1, doneAreas: [] },

    exists: function () {
      return FileTools.exists(this._path);
    },
      
    create: function () {
      if (!FileTools.exists("data/" + me.profile)) {
        let folder = dopen("data");
        folder.create(me.profile);
      }

      const obj = Object.assign({}, this._default);

      if (me.gamename) {
        obj.gamename = me.gamename;
      }

      if (me.gamepassword) {
        obj.gamepassword = me.gamepassword;
      }

      FileAction.write(this._path, JSON.stringify(obj));
      return obj;
    },

    /** @returns {OrgTorchDataObject} */
    read: function () {
      try {
        return FileAction.parse(this._path);
      } catch (e) {
        return this._default;
      }
    },

    /** @param {Partial<OrgTorchDataObject>} newData */
    update: function (newData) {
      let data = this.read();
      Object.assign(data, newData);
      FileTools.writeText(this._path, JSON.stringify(data));
    },

    remove: function () {
      return FileTools.remove(this._path);
    }
  };

  module.exports = OrgTorchData;
})(module);
