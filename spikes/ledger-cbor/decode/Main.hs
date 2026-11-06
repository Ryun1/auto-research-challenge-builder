{-# LANGUAGE DataKinds #-}
{-# LANGUAGE OverloadedStrings #-}
{-# LANGUAGE TypeApplications #-}

-- Spike: decode real mainnet Conway transactions with cardano-ledger's own
-- decoder (the Annotator path the node uses), check each re-encodes to its
-- exact bytes, then decode the whole corpus N more times for measurement.
--
--   ledger-cbor-spike CORPUS.hex ITERS
module Main (main) where

import Cardano.Ledger.Binary (decCBOR, decodeFullAnnotator, natVersion, serialize')
import Cardano.Ledger.Conway (ConwayEra)
import Cardano.Ledger.MempoolTx (MempoolTx, unwrapMempoolTx)
import Control.DeepSeq (rnf)
import Control.Monad (forM_, replicateM_, unless)
import qualified Data.ByteString as BS
import qualified Data.ByteString.Base16 as B16
import qualified Data.ByteString.Char8 as BS8
import qualified Data.ByteString.Lazy as BSL
import System.Environment (getArgs)
import System.Exit (exitFailure)

decodeTx :: BS.ByteString -> Either String (MempoolTx ConwayEra)
decodeTx bs =
  either (Left . show) Right $
    decodeFullAnnotator (natVersion @11) "MempoolTx" decCBOR (BSL.fromStrict bs)

main :: IO ()
main = do
  [path, itersS] <- getArgs
  raw <- BS.readFile path
  txs <- mapM (either fail pure . B16.decode) (filter (not . BS.null) (BS8.lines raw))
  let results = [(b, decodeTx b) | b <- txs]
      fails = [e | (_, Left e) <- results]
      mismatches = length [() | (b, Right t) <- results, serialize' (natVersion @11) t /= b]
  putStrLn $
    "txs=" <> show (length txs)
      <> " bytes=" <> show (sum (map BS.length txs))
      <> " decode_fail=" <> show (length fails)
      <> " roundtrip_mismatch=" <> show mismatches
  mapM_ putStrLn (take 3 fails)
  replicateM_ (read itersS) $
    forM_ txs $ \b -> case decodeTx b of
      Left e -> fail e
      Right t -> pure $! rnf (unwrapMempoolTx t)
  unless (null fails && mismatches == 0) exitFailure
