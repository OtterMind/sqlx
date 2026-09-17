export class UsageError extends Error {}

export class InstallError extends Error {
  constructor(stage, message, options) {
    super(message, options);
    this.stage = stage;
  }
}
