import { createInitialPosState, reducePosState, } from "@hellskitchen/shared";
class StateStore {
    state = createInitialPosState();
    getState() {
        return this.state;
    }
    dispatch(input) {
        const action = {
            ...input,
            id: crypto.randomUUID(),
            at: new Date().toISOString(),
        };
        this.state = reducePosState(this.state, action);
        return this.state;
    }
    reset() {
        this.state = createInitialPosState();
        return this.state;
    }
}
export const store = new StateStore();
