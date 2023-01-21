type Constructor<T> = new (...args: any[]) => T;

export default function includesType(array: readonly any[], targetType: Constructor<any>): boolean {
    if (array.length === 0) return false;

    const filteredArray = array.filter(element => element instanceof targetType);
    return filteredArray.length > 0; // it checks if the filtered array has an element which is an instanceOf targetType
}