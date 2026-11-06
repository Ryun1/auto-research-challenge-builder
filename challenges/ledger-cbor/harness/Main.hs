{-# LANGUAGE DataKinds #-}
{-# LANGUAGE OverloadedStrings #-}
{-# LANGUAGE ScopedTypeVariables #-}
{-# LANGUAGE TypeApplications #-}

-- Judge harness. Built twice from this same source: against the pristine
-- cardano-ledger-binary (reference) and against candidate/ (candidate).
--
--   harness digest FILE       one line per input: "ok <show-hash> <bytes-hash>", "err" or "exc"
--   harness decode FILE REPS  decode every input REPS times (0 = read only); prints decode seconds
--
-- FILE holds one hex-encoded transaction per line (an empty line is an empty input).
module Main (main) where

import Cardano.Crypto.Hash (Blake2b_256, hashToStringAsHex, hashWith)
import Cardano.Ledger.Binary (decCBOR, decodeFullAnnotator, natVersion, serialize')
import Cardano.Ledger.Conway (ConwayEra)
import Cardano.Ledger.MempoolTx (MempoolTx, unwrapMempoolTx)
import Control.DeepSeq (force, rnf)
import Control.Exception (SomeException, evaluate, try)
import Control.Monad (forM_, replicateM_)
import qualified Data.ByteString as BS
import qualified Data.ByteString.Base16 as B16
import qualified Data.ByteString.Char8 as BS8
import qualified Data.ByteString.Lazy as BSL
import GHC.Clock (getMonotonicTime)
import System.Environment (getArgs)

-- Mainnet's current protocol version (Conway, PV 11).
decodeTx :: BS.ByteString -> Either String (MempoolTx ConwayEra)
decodeTx bs =
  either (Left . show) Right $
    decodeFullAnnotator (natVersion @11) "MempoolTx" decCBOR (BSL.fromStrict bs)

hash :: BS.ByteString -> String
hash = hashToStringAsHex . hashWith @Blake2b_256 id

digestLine :: BS.ByteString -> String
digestLine b = case decodeTx b of
  Left _ -> "err"
  Right t ->
    "ok " <> hash (BS8.pack (show (unwrapMempoolTx t)))
      <> " " <> hash (serialize' (natVersion @11) t)

readInputs :: FilePath -> IO [BS.ByteString]
readInputs path = do
  raw <- BS.readFile path
  mapM (either fail pure . B16.decode) (BS8.lines raw)

main :: IO ()
main = do
  args <- getArgs
  case args of
    ["digest", path] -> do
      txs <- readInputs path
      forM_ txs $ \b -> do
        -- a decoder that throws instead of returning Left is still compared
        r <- try (evaluate (force (digestLine b)))
        putStrLn $ either (\(_ :: SomeException) -> "exc") id r
    ["decode", path, reps] -> do
      txs <- readInputs path
      BS.length (BS.concat txs) `seq` pure ()
      t0 <- getMonotonicTime
      replicateM_ (read reps) $
        forM_ txs $ \b -> case decodeTx b of
          Left e -> fail e
          Right t -> pure $! rnf (unwrapMempoolTx t)
      t1 <- getMonotonicTime
      print (t1 - t0)
    _ -> fail "usage: harness digest FILE | harness decode FILE REPS"
