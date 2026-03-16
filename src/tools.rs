use rquickjs::Ctx;

pub fn setup_tools(ctx: Ctx<'_>) -> rquickjs::Result<()> {
    ctx.eval::<(), _>(r#"
class ToolNotFoundError extends Error {
  constructor(name) {
    super('Tool not found: ' + name);
    this.name = 'ToolNotFoundError';
    this.toolName = name;
  }
}
class ToolValidationError extends Error {
  constructor(message) {
    super(message);
    this.name = 'ToolValidationError';
  }
}
class ToolRegistrationError extends Error {
  constructor(message) {
    super(message);
    this.name = 'ToolRegistrationError';
  }
}

class Tools {
  constructor(toolDefs) {
    this._tools = {};
    if (Array.isArray(toolDefs)) {
      for (var i = 0; i < toolDefs.length; i++) {
        this.addTool(toolDefs[i]);
      }
    }
  }

  addTool(def) {
    if (!def || typeof def.name !== 'string' || !def.name)
      throw new ToolRegistrationError('Tool definition must have a non-empty name string');
    if (this._tools[def.name])
      throw new ToolRegistrationError('Tool already registered: ' + def.name + '. Use removeTool() first.');
    if (typeof def.handler !== 'function')
      throw new ToolRegistrationError('Tool "' + def.name + '" must have a handler function');
    this._tools[def.name] = {
      name: def.name,
      description: def.description || '',
      parameters: def.parameters || { type: 'object', properties: {}, required: [] },
      handler: def.handler
    };
  }

  removeTool(name) { delete this._tools[name]; }

  hasTool(name) { return Object.prototype.hasOwnProperty.call(this._tools, name); }

  listTools() {
    return Object.keys(this._tools).map(function(name) {
      var t = this._tools[name];
      return { type: 'function', function: { name: t.name, description: t.description, parameters: t.parameters } };
    }, this);
  }

  async call(nameOrToolCall, args) {
    if (nameOrToolCall !== null && typeof nameOrToolCall === 'object') {
      var tc = nameOrToolCall;
      var fnName = tc.function && tc.function.name;
      var rawArgs = tc.function && tc.function.arguments;
      var callId = tc.id;
      var parsedArgs;
      if (typeof rawArgs === 'string') {
        try { parsedArgs = JSON.parse(rawArgs); }
        catch (e) { throw new ToolValidationError('Malformed JSON arguments for tool "' + fnName + '": ' + e.message); }
      } else {
        parsedArgs = (rawArgs !== null && typeof rawArgs === 'object') ? rawArgs : {};
      }
      try {
        var result = await this._invoke(fnName, parsedArgs);
        return { tool_call_id: callId, name: fnName, result: result };
      } catch (e) {
        if (e instanceof ToolNotFoundError || e instanceof ToolValidationError) throw e;
        return { tool_call_id: callId, name: fnName, error: true, result: e.message || String(e) };
      }
    }
    return this._invoke(nameOrToolCall, args || {});
  }

  async callMany(toolCalls) {
    var results = [];
    for (var i = 0; i < toolCalls.length; i++) {
      results.push(await this.call(toolCalls[i]));
    }
    return results;
  }

  async _invoke(name, args) {
    if (!this.hasTool(name)) throw new ToolNotFoundError(name);
    var tool = this._tools[name];
    var props = (tool.parameters && tool.parameters.properties) ? tool.parameters.properties : {};
    var required = (tool.parameters && Array.isArray(tool.parameters.required)) ? tool.parameters.required : [];
    for (var i = 0; i < required.length; i++) {
      if (!Object.prototype.hasOwnProperty.call(args, required[i]))
        throw new ToolValidationError('Missing required parameter "' + required[i] + '" for tool "' + name + '"');
    }
    var filtered = {};
    var propKeys = Object.keys(props);
    for (var j = 0; j < propKeys.length; j++) {
      var k = propKeys[j];
      if (Object.prototype.hasOwnProperty.call(args, k)) filtered[k] = args[k];
    }
    return await tool.handler(filtered);
  }
}

globalThis.Tools = Tools;
globalThis.ToolNotFoundError = ToolNotFoundError;
globalThis.ToolValidationError = ToolValidationError;
globalThis.ToolRegistrationError = ToolRegistrationError;
    "#)?;
    Ok(())
}
