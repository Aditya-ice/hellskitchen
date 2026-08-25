import {
  createInitialPosState,
  type PosState,
  reducePosState,
  type SharedAction,
  type SharedActionInput,
} from "@hellskitchen/shared";

class StateStore {
  // Start from wall-clock time so revisions remain newer across process restarts.
  private state: PosState = createInitialPosState(Date.now());

  getState(): PosState {
    return this.state;
  }

  dispatch(input: SharedActionInput): PosState {
    const action: SharedAction = {
      ...input,
      id: crypto.randomUUID(),
      at: new Date().toISOString(),
    } as SharedAction;

    this.state = reducePosState(this.state, action);
    return this.state;
  }

  reset(): PosState {
    this.state = createInitialPosState(this.state.version + 1);
    return this.state;
  }
}

export const store = new StateStore();
