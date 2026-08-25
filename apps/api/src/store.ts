import {
  createInitialPosState,
  type PosState,
  reducePosState,
  type SharedAction,
  type SharedActionInput,
} from "@hellskitchen/shared";

class StateStore {
  private state: PosState = createInitialPosState();

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
    this.state = createInitialPosState();
    return this.state;
  }
}

export const store = new StateStore();
