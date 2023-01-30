import { useEffect, useState } from "react";

export default function useWithLoading<T>(
  promise: Promise<T>,
  onPromiseResolved: (value: T) => any = () => {},
  dependencies: any[] = [],
) {
  const [isLoading, setIsLoading] = useState(true);

  useEffect(() => {
    promise.then(onPromiseResolved).finally(() => {
      setIsLoading(false);
    });
  }, [...dependencies]);

  return [isLoading];
}
