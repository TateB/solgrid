use criterion::{criterion_group, criterion_main, Criterion};
use solgrid_config::Config;
use solgrid_linter::LintEngine;
use std::path::Path;

const SAMPLE_CONTRACT: &str = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {Ownable} from "@openzeppelin/contracts/access/Ownable.sol";

/// @title TokenVault
/// @author solgrid-bench
/// @notice A vault for depositing and withdrawing ERC20 tokens
contract TokenVault is Ownable {
    /// @notice Emitted when tokens are deposited
    /// @param user The depositor
    /// @param token The token address
    /// @param amount The amount deposited
    event Deposit(address indexed user, address indexed token, uint256 amount);

    /// @notice Emitted when tokens are withdrawn
    /// @param user The withdrawer
    /// @param token The token address
    /// @param amount The amount withdrawn
    event Withdrawal(address indexed user, address indexed token, uint256 amount);

    error InsufficientBalance(uint256 requested, uint256 available);
    error ZeroAmount();
    error InvalidToken();

    struct UserInfo {
        uint256 depositedAmount;
        uint256 lastDepositTime;
        uint256 rewardDebt;
        bool isActive;
    }

    mapping(address => mapping(address => UserInfo)) public userInfo;
    mapping(address => uint256) public totalDeposited;
    mapping(address => bool) public supportedTokens;

    uint256 public constant MAX_FEE_RATE = 1000;
    uint256 public feeRate;
    uint256 public immutable DEPLOYMENT_TIME;
    address[] private _supportedTokenList;

    /// @notice Creates the vault
    /// @param initialFeeRate The initial fee rate in basis points
    constructor(uint256 initialFeeRate) Ownable(msg.sender) {
        require(initialFeeRate <= MAX_FEE_RATE, "Fee too high");
        feeRate = initialFeeRate;
        DEPLOYMENT_TIME = block.timestamp;
    }

    /// @notice Deposit tokens into the vault
    /// @param token The token to deposit
    /// @param amount The amount to deposit
    function deposit(address token, uint256 amount) external {
        if (amount == 0) revert ZeroAmount();
        if (!supportedTokens[token]) revert InvalidToken();

        UserInfo storage info = userInfo[msg.sender][token];
        info.depositedAmount += amount;
        info.lastDepositTime = block.timestamp;
        info.isActive = true;
        totalDeposited[token] += amount;

        IERC20(token).transferFrom(msg.sender, address(this), amount);
        emit Deposit(msg.sender, token, amount);
    }

    /// @notice Withdraw tokens from the vault
    /// @param token The token to withdraw
    /// @param amount The amount to withdraw
    function withdraw(address token, uint256 amount) external {
        if (amount == 0) revert ZeroAmount();

        UserInfo storage info = userInfo[msg.sender][token];
        if (info.depositedAmount < amount) {
            revert InsufficientBalance(amount, info.depositedAmount);
        }

        uint256 fee = (amount * feeRate) / 10000;
        uint256 netAmount = amount - fee;

        info.depositedAmount -= amount;
        if (info.depositedAmount == 0) {
            info.isActive = false;
        }
        totalDeposited[token] -= amount;

        IERC20(token).transfer(msg.sender, netAmount);
        if (fee > 0) {
            IERC20(token).transfer(owner(), fee);
        }
        emit Withdrawal(msg.sender, token, netAmount);
    }

    /// @notice Add a supported token
    /// @param token The token address to support
    function addSupportedToken(address token) external onlyOwner {
        if (token == address(0)) revert InvalidToken();
        if (!supportedTokens[token]) {
            supportedTokens[token] = true;
            _supportedTokenList.push(token);
        }
    }

    /// @notice Get the number of supported tokens
    /// @return The count of supported tokens
    function supportedTokenCount() external view returns (uint256) {
        return _supportedTokenList.length;
    }

    /// @notice Calculate the fee for a given amount
    /// @param amount The amount
    /// @return The fee amount
    function calculateFee(uint256 amount) public view returns (uint256) {
        return (amount * feeRate) / 10000;
    }
}
"#;

fn bench_lint_contract(c: &mut Criterion) {
    let engine = LintEngine::new();
    let config = Config::default();
    let path = Path::new("bench.sol");

    c.bench_function("lint_contract", |b| {
        b.iter(|| engine.lint_source(SAMPLE_CONTRACT, path, &config))
    });
}

fn bench_lint_and_fix(c: &mut Criterion) {
    let engine = LintEngine::new();
    let config = Config::default();
    let path = Path::new("bench.sol");

    c.bench_function("lint_and_fix_contract", |b| {
        b.iter(|| engine.fix_source(SAMPLE_CONTRACT, path, &config, false))
    });
}

criterion_group!(benches, bench_lint_contract, bench_lint_and_fix);
criterion_main!(benches);
