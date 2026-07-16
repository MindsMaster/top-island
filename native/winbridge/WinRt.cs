using System;
using System.Runtime.ExceptionServices;
using Windows.Foundation;

namespace TopIsland.WinBridge
{
    internal static class WinRt
    {
        public static T Await<T>(IAsyncOperation<T> op, int timeoutMs = 8000)
        {
            var task = op.AsTask();
            try
            {
                if (!task.Wait(timeoutMs)) throw new TimeoutException("WinRT operation timeout");
            }
            catch (AggregateException ex) when (ex.InnerException != null)
            {
                ExceptionDispatchInfo.Capture(ex.InnerException).Throw();
            }
            return task.Result;
        }
    }
}
